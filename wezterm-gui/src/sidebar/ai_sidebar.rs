//! AI assistant sidebar implementation
//!
//! This module provides the main AI sidebar interface for WezTerm, including:
//! - Activity log showing commands, chats, and suggestions
//! - Agent status display (idle, thinking, gathering data, needs approval)
//! - Input field for user interactions
//! - Suggestion cards with "more..." expansion to modals
//! - Modal overlays for expanded content
//!
//! The sidebar manages its own state and rendering, integrating with the
//! terminal window through event handlers and the rendering pipeline.

use super::components::markdown::{CodeBlockContainer, CodeBlockRegistry};
use super::components::{
    Card, CardState, Chip, ChipSize, ChipStyle, MarkdownRenderer, Modal, ModalContent,
    ModalManager, ModalSize, MultilineTextInput, ScrollableContainer, ScrollbarInfo,
    SuggestionModal,
};
use super::{Sidebar, SidebarConfig, SidebarFonts, SidebarPosition};
use crate::color::LinearRgba;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
};
use crate::termwindow::render::scrollbar_renderer::{ScrollbarOrientation, ScrollbarRenderer};
use crate::termwindow::UIItemType;
use anyhow::Result;
use config::{Dimension, DimensionContext};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use termwiz::input::KeyCode;
use wezterm_font::{FontConfiguration, LoadedFont};
use wezterm_term::KeyModifiers;
use window::{MouseEvent, MouseEventKind as WMEK, MousePress, PixelUnit, RectF};

// Virtual scrolling constants
const RENDER_MARGIN: f32 = 200.0; // Pixels to render beyond viewport
const WIDTH_CHANGE_THRESHOLD: f32 = 5.0; // Pixels of width change to trigger cache clear
const HEIGHT_CHANGE_HYSTERESIS: f32 = 2.0; // Minimum height change to update cache

/// Tracks height measurement state for activity items
#[derive(Debug, Clone, Default)]
struct HeightTracker {
    /// For tall items - scroll offset when top edge entered viewport
    top_entered_at: Option<f32>,
    
    /// For tall items - scroll offset when bottom edge entered viewport  
    bottom_entered_at: Option<f32>,
    
    /// Whether we've seen this item's full height (unclipped)
    seen_full_height: bool,
    
    /// Final measured height (from rendering or scroll tracking)
    measured_height: Option<f32>,
}

/// Visual anchor for maintaining scroll position stability
#[derive(Debug, Clone)]
struct VisualAnchor {
    /// Index of the anchored item in filtered items
    item_index: usize,
    /// Offset within the item (0 = top of item)
    offset_within_item: f32,
    /// Position on screen where anchor appears (0 = top of viewport)
    screen_position: f32,
}

// Character width estimation for suggestion cards
// This is tuned specifically for the sidebar's font (Roboto)
// Activity log uses 0.6 which is more conservative
const SUGGESTION_CHAR_WIDTH_MULTIPLIER: f32 = 0.4; // Try to get close to 2 full lines (but not beyond)

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    Idle,
    Thinking,
    GatheringData,
    NeedsApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityFilter {
    All,
    Commands,
    Chat,
    Suggestions,
}

#[derive(Debug, Clone)]
pub enum ActivityItem {
    Command {
        id: String,
        command: String,
        output: Option<String>,
        pane_id: Option<String>,
        status: CommandStatus,
        timestamp: SystemTime,
        expanded: bool,
    },
    Chat {
        id: String,
        message: String,
        is_user: bool,
        timestamp: SystemTime,
    },
    Suggestion {
        id: String,
        title: String,
        content: String,
        timestamp: SystemTime,
        is_current: bool,
    },
    Goal {
        id: String,
        text: String,
        timestamp: SystemTime,
        is_current: bool,
        is_confirmed: bool,
    },
}

impl super::components::scrollable_v2::ScrollableItem for ActivityItem {
    fn id(&self) -> String {
        match self {
            ActivityItem::Command { id, .. } => id.clone(),
            ActivityItem::Chat { id, .. } => id.clone(),
            ActivityItem::Suggestion { id, .. } => id.clone(),
            ActivityItem::Goal { id, .. } => id.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CommandStatus {
    Running,
    Success,
    Failed(i32),
}

pub struct CurrentGoal {
    text: String,
    is_ai_inferred: bool,
    is_confirmed: bool,
    is_editing: bool,
    edit_text: String,
}

#[derive(Clone)]
pub struct CurrentSuggestion {
    pub title: String,
    pub content: String,
    pub has_action: bool,
    pub action_type: Option<String>, // "run", "dismiss", etc
}

/// Main AI assistant sidebar implementation
///
/// Manages the state and rendering of the AI sidebar, including:
/// - Activity log with filtering
/// - Agent status and modes
/// - User input handling
/// - Suggestion display with modal expansion
/// - Scroll state and interaction
pub struct AiSidebar {
    config: SidebarConfig,
    visible: bool,
    width: u16,

    // UI State
    agent_mode: AgentMode,
    agent_mode_enabled: bool,
    high_risk_mode_enabled: bool,
    pub activity_filter: ActivityFilter,

    // Data
    pub current_goal: Option<CurrentGoal>,
    pub current_suggestion: Option<CurrentSuggestion>,
    activity_log: Vec<ActivityItem>,

    // UI Components
    chat_input: MultilineTextInput,

    // Height caching for virtual scrolling
    activity_log_height_cache: HashMap<String, f32>,
    
    // Height tracking state for each item
    height_trackers: HashMap<String, HeightTracker>,

    // Last known width for cache invalidation
    activity_log_last_width: Option<f32>,

    // Visible range for virtual scrolling
    activity_log_visible_range: Range<usize>,

    // Scrollbar info for external rendering
    activity_log_scrollbar: Option<ScrollbarInfo>,

    // Scrollbar renderer for handling events
    activity_log_scrollbar_renderer: Option<ScrollbarRenderer>,

    // Scrollbar bounds for hit testing
    activity_log_scrollbar_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Scroll state
    activity_log_scroll_offset: f32,
    
    // Visual anchor for maintaining position during height changes
    visual_anchor: Option<VisualAnchor>,

    // UI element bounds for hit testing
    filter_chip_bounds: Vec<(ActivityFilter, euclid::Rect<f32, window::PixelUnit>)>,
    more_link_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Sidebar position for coordinate conversion
    sidebar_x_position: f32,

    // Modal management
    modal_manager: ModalManager,

    // Code block registry for horizontal scrolling
    pub code_block_registry: Option<CodeBlockRegistry>,
}

impl AiSidebar {
    pub fn new(config: SidebarConfig) -> Self {
        Self {
            width: config.width,
            visible: config.show_on_startup,
            config,
            agent_mode: AgentMode::Idle,
            agent_mode_enabled: false,
            high_risk_mode_enabled: false,
            activity_filter: ActivityFilter::All,
            current_goal: None,
            current_suggestion: None,
            activity_log: Vec::new(),
            chat_input: MultilineTextInput::new(3).with_placeholder("Type a message..."),
            activity_log_height_cache: HashMap::new(),
            height_trackers: HashMap::new(),
            activity_log_last_width: None,
            activity_log_visible_range: 0..0,
            activity_log_scrollbar: None,
            activity_log_scrollbar_renderer: None,
            activity_log_scrollbar_bounds: None,
            activity_log_scroll_offset: 0.0,
            visual_anchor: None,
            filter_chip_bounds: Vec::new(),
            more_link_bounds: None,
            sidebar_x_position: 0.0,
            modal_manager: ModalManager::new(),
            code_block_registry: Some(Arc::new(Mutex::new(HashMap::new()))),
        }
    }

    // Mock data for development
    pub fn populate_mock_data(&mut self) {
        // Set a current goal
        self.current_goal = Some(CurrentGoal {
            text: "Fix the build errors in the project".to_string(),
            is_ai_inferred: true,
            is_confirmed: false,
            is_editing: false,
            edit_text: String::new(),
        });

        // Set a current suggestion with very long content to test scrolling
        let test_content = r#"It looks like the linker couldn't find OpenSSL. This is a common issue when building projects that depend on OpenSSL for cryptographic functionality. Let me provide a comprehensive guide to resolving this issue.

## Quick Solution

Run the following command to install OpenSSL:

```bash
brew install openssl@3
```

## If That Doesn't Work

You may need to set environment variables to help the build system find OpenSSL:

```bash
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl@3/lib/pkgconfig"
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
```

## Common Issues

1. **Wrong OpenSSL version**: Some projects require openssl@1.1 instead of openssl@3
2. **Multiple OpenSSL installations**: Check `brew list | grep openssl` to see all versions
3. **Architecture mismatch**: On M1 Macs, ensure you're using the right architecture
4. **Missing pkg-config**: Install with `brew install pkg-config`
5. **Incorrect paths**: Verify paths with `brew --prefix openssl@3`

## Detailed Troubleshooting Steps

### Step 1: Check Current Installation
First, let's check what OpenSSL versions you have installed:

```bash
brew list | grep openssl
ls -la /opt/homebrew/opt/ | grep openssl
which openssl
openssl version
```

### Step 2: Clean Installation
If you have conflicts, clean up first:

```bash
brew uninstall --ignore-dependencies openssl@3
brew uninstall --ignore-dependencies openssl@1.1
brew cleanup
```

### Step 3: Fresh Install
Install the required version:

```bash
brew install openssl@3
brew link openssl@3 --force
```

### Step 4: Verify Installation
Check that everything is properly installed:

```bash
brew test openssl@3
pkg-config --libs openssl
```

### Step 5: Configure Your Shell
Add these to your shell configuration file (~/.zshrc or ~/.bashrc):

```bash
# OpenSSL Configuration
export PATH="/opt/homebrew/opt/openssl@3/bin:$PATH"
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl@3/lib/pkgconfig"
```

### Step 6: Alternative Solutions

#### Using MacPorts
If Homebrew doesn't work, try MacPorts:

```bash
sudo port install openssl
sudo port select --set openssl openssl3
```

#### Building from Source
As a last resort, build OpenSSL from source:

```bash
wget https://www.openssl.org/source/openssl-3.0.7.tar.gz
tar -xf openssl-3.0.7.tar.gz
cd openssl-3.0.7
./config --prefix=/usr/local/openssl --openssldir=/usr/local/openssl
make
sudo make install
```

## Platform-Specific Notes

### macOS Monterey and Later
Apple has deprecated OpenSSL in favor of their own crypto libraries. You may need to:

1. Disable System Integrity Protection (not recommended)
2. Use a different crypto library
3. Explicitly specify OpenSSL paths in your build configuration

### M1/M2 Mac Considerations
On Apple Silicon, paths differ:
- Intel: `/usr/local/opt/openssl@3`
- Apple Silicon: `/opt/homebrew/opt/openssl@3`

## Related Issues
- libssl-dev on Linux: `sudo apt-get install libssl-dev`
- Windows: Use vcpkg or download prebuilt binaries
- Docker: Add `RUN apk add --no-cache openssl-dev` to Dockerfile

This should resolve most OpenSSL-related build issues. If problems persist, check your project's specific requirements.

If you're still having issues:

1. Clean your build directory: `make clean`
2. Check your PATH: `echo $PATH`
3. Verify OpenSSL installation: `brew info openssl@3`
4. Try linking manually: `brew link openssl@3 --force`

## References

- [Homebrew OpenSSL Formula](https://formulae.brew.sh/formula/openssl@3)
- [Common macOS linking issues](https://github.com/openssl/openssl/issues)

This should resolve your OpenSSL linking error. If problems persist, check your project's specific build documentation."#;

        self.current_suggestion = Some(CurrentSuggestion {
            title: "Install missing dependency".to_string(),
            content: test_content.to_string(),
            has_action: true,
            action_type: Some("run".to_string()),
        });

        // Add some activity items
        let now = SystemTime::now();
        self.activity_log.push(ActivityItem::Command {
            id: "cmd1".to_string(),
            command: "make (~/project)".to_string(),
            output: Some("Error: OpenSSL not found".to_string()),
            pane_id: Some("pane1".to_string()),
            status: CommandStatus::Failed(1),
            timestamp: now - Duration::from_secs(60),
            expanded: true, // Make it expanded to see if that's the tall content
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat1".to_string(),
            message: "I'm trying to compile my Rust project but getting linker errors about OpenSSL. I've tried installing it before but it doesn't seem to be working. Can you help me understand what's going wrong and how to fix it properly?".to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(30),
        });

        // Add AI response with long markdown content to test text wrapping
        self.activity_log.push(ActivityItem::Chat {
            id: "chat2".to_string(),
            message: r#"I see you're getting an **OpenSSL error**. This is a very common issue when building projects that depend on OpenSSL for cryptographic functionality. Let me provide you with a comprehensive guide to resolve this issue on macOS.

## Quick Solution (Try This First)

The fastest way to resolve this is usually:

1. First, *check* if OpenSSL is installed:
   ```bash
   brew list openssl
   brew list | grep openssl
   ```

2. If **not installed**, run:
   ```bash
   brew install openssl@3
   # or for older projects:
   brew install openssl@1.1
   ```

3. Then set the environment variables:
   ```bash
   export OPENSSL_DIR=$(brew --prefix openssl)
   export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"
   export LDFLAGS="-L$OPENSSL_DIR/lib"
   export CPPFLAGS="-I$OPENSSL_DIR/include"
   # This is a very long line that should definitely trigger horizontal scrolling in the code block - it contains many characters and should exceed the width of the sidebar
   ```

4. Try running `make` again.

## Detailed Troubleshooting

If the quick solution doesn't work, here are more comprehensive steps:

### Step 1: Verify Your System
First, let's understand your environment:
```bash
# Check macOS version
sw_vers -productVersion

# Check architecture (Intel vs Apple Silicon)
uname -m

# Check Homebrew installation
brew --version
brew config
```

### Step 2: Clean Up Existing Installations
Sometimes conflicts arise from multiple OpenSSL installations:
```bash
# List all OpenSSL installations
brew list | grep openssl
ls -la /usr/local/opt/ | grep openssl
ls -la /opt/homebrew/opt/ | grep openssl

# If you have conflicts, uninstall all versions
brew uninstall --ignore-dependencies openssl@3
brew uninstall --ignore-dependencies openssl@1.1
brew uninstall --ignore-dependencies openssl
```

### Step 3: Install the Correct Version
Different projects require different OpenSSL versions:
```bash
# For modern projects (OpenSSL 3.x)
brew install openssl@3

# For older projects (OpenSSL 1.1)
brew install openssl@1.1

# Force link if needed
brew link openssl@3 --force
```

### Step 4: Configure pkg-config
The pkg-config tool helps compilers find libraries:
```bash
# Install pkg-config if missing
brew install pkg-config

# Verify it can find OpenSSL
pkg-config --modversion openssl
pkg-config --libs openssl
pkg-config --cflags openssl
```

### Alternative Solution
If the above doesn't work, you might need to:
```bash
# Install pkg-config
brew install pkg-config

# Or try using the system's built-in LibreSSL
export LDFLAGS="-L/usr/lib"
export CPPFLAGS="-I/usr/include"
```

## Platform-Specific Considerations

### Apple Silicon (M1/M2) Macs
Paths differ on Apple Silicon:
- Intel Macs: `/usr/local/opt/openssl`
- Apple Silicon: `/opt/homebrew/opt/openssl`

### macOS Ventura and Later
Apple has deprecated OpenSSL in favor of their own crypto libraries, which can cause additional complications.

This comprehensive guide should resolve most OpenSSL linking issues on macOS!"#.to_string(),
            is_user: false,
            timestamp: now - Duration::from_secs(20),
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat3".to_string(),
            message: "Great! That worked. Now I'm seeing some warnings about deprecated functions."
                .to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(10),
        });

        // Add a Python example with indentation to test code block rendering
        self.activity_log.push(ActivityItem::Chat {
            id: "chat4".to_string(),
            message: r#"Here's a Python example showing proper error handling with indentation:

```python
def process_data(filename):
    """Process data from a file with proper error handling."""
    try:
        with open(filename, 'r') as file:
            data = file.read()
            # Process each line
            for line in data.splitlines():
                if line.strip():  # Skip empty lines
                    result = parse_line(line)
                    if result:
                        yield result
    except FileNotFoundError:
        print(f"Error: File '{filename}' not found")
        return None
    except PermissionError:
        print(f"Error: Permission denied for '{filename}'")
        return None
    finally:
        print("Processing complete")
```

This example demonstrates:
- Function definition with docstring
- Context manager (`with` statement)
- Nested indentation levels (up to 5 levels deep)
- Error handling with multiple `except` blocks
- The `finally` clause for cleanup"#
                .to_string(),
            is_user: false,
            timestamp: now - Duration::from_secs(5),
        });

        // Add more mock items to test scrolling
        for i in 0..20 {
            if i % 3 == 0 {
                self.activity_log.push(ActivityItem::Command {
                    id: format!("cmd{}", i + 10),
                    command: format!("test command {}", i),
                    output: Some(format!("Output for command {}", i)),
                    pane_id: Some("pane1".to_string()),
                    status: if i % 2 == 0 {
                        CommandStatus::Success
                    } else {
                        CommandStatus::Failed(1)
                    },
                    timestamp: now - Duration::from_secs(300 + i * 60),
                    expanded: false,
                });
            } else {
                self.activity_log.push(ActivityItem::Chat {
                    id: format!("chat{}", i + 10),
                    message: format!(
                        "Test message {} from {}",
                        i,
                        if i % 2 == 0 { "user" } else { "AI" }
                    ),
                    is_user: i % 2 == 0,
                    timestamp: now - Duration::from_secs(300 + i * 60),
                });
            }
        }

        self.agent_mode = AgentMode::Thinking;

        // Clear code block registry since we've replaced all content
        self.clear_code_block_registry();
    }

    fn render_header(&self, fonts: &SidebarFonts) -> Element {
        let title = Element::new(
            &fonts.heading,
            ElementContent::Text("CLiBuddy AI".to_string()),
        )
        .colors(ElementColors {
            text: LinearRgba::with_components(0.95, 0.95, 0.95, 1.0).into(),
            ..Default::default()
        })
        .padding(BoxDimension {
            left: Dimension::Pixels(16.0),
            top: Dimension::Pixels(12.0),
            bottom: Dimension::Pixels(12.0),
            right: Dimension::Pixels(16.0),
        });

        Element::new(&fonts.heading, ElementContent::Children(vec![title]))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.08, 0.08, 0.1, 1.0).into(),
                ..Default::default()
            })
            .border(BoxDimension {
                bottom: Dimension::Pixels(1.0),
                ..Default::default()
            })
            .colors(ElementColors {
                border: BorderColor::new(LinearRgba::with_components(0.2, 0.2, 0.25, 0.5)),
                bg: LinearRgba::with_components(0.08, 0.08, 0.1, 1.0).into(),
                ..Default::default()
            })
    }

    fn render_filter_chips(&mut self, fonts: &SidebarFonts) -> Element {
        let filters = vec![
            ("All", ActivityFilter::All),
            ("Commands", ActivityFilter::Commands),
            ("Chat", ActivityFilter::Chat),
            ("Suggestions", ActivityFilter::Suggestions),
        ];

        let chips: Vec<Element> = filters
            .into_iter()
            .map(|(label, filter)| {
                let is_selected = self.activity_filter == filter;
                let style = if is_selected {
                    ChipStyle::Primary
                } else {
                    ChipStyle::Default
                };

                Chip::new(label.to_string())
                    .with_style(style)
                    .with_size(ChipSize::Small)
                    .clickable(true)
                    .selected(is_selected)
                    .with_item_type(crate::termwindow::UIItemType::SidebarFilterChip(filter))
                    .render(&fonts.body)
            })
            .collect();

        Element::new(&fonts.body, ElementContent::Children(chips))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(8.0),
            })
    }

    fn render_status_chip(&self, fonts: &SidebarFonts) -> Element {
        let (label, style, icon) = match self.agent_mode {
            AgentMode::Idle => ("Idle", ChipStyle::Default, "○"),
            AgentMode::Thinking => ("Thinking", ChipStyle::Info, "◐"),
            AgentMode::GatheringData => ("Gathering Data", ChipStyle::Warning, "◑"),
            AgentMode::NeedsApproval => ("Needs Approval", ChipStyle::Error, "⚠"),
        };

        let chip = Chip::new(label.to_string())
            .with_style(style)
            .with_size(ChipSize::Medium)
            .with_icon(icon.to_string())
            .render(&fonts.body);

        Element::new(&fonts.body, ElementContent::Children(vec![chip]))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(4.0),
            })
    }

    fn render_current_goal(&self, fonts: &SidebarFonts) -> Option<Element> {
        let goal = self.current_goal.as_ref()?;

        let mut content = vec![];

        // Goal text
        let goal_text = if goal.is_editing {
            // Show edit input
            Element::new(
                &fonts.body,
                ElementContent::Text(format!("{}_", &goal.edit_text)),
            )
            .colors(ElementColors {
                text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                bg: LinearRgba::with_components(0.15, 0.15, 0.17, 1.0).into(),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        } else {
            Element::new(&fonts.body, ElementContent::WrappedText(goal.text.clone()))
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                    ..Default::default()
                })
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        };
        content.push(goal_text);

        // Action buttons
        let mut actions = vec![];

        if goal.is_ai_inferred && !goal.is_confirmed && !goal.is_editing {
            let confirm_btn = Chip::new("✓".to_string())
                .with_style(ChipStyle::Success)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            actions.push(confirm_btn);
        }

        if !goal.is_editing {
            let edit_btn = Chip::new("✎".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            actions.push(edit_btn);
        } else {
            let save_btn = Chip::new("Save".to_string())
                .with_style(ChipStyle::Primary)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            let cancel_btn = Chip::new("Cancel".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            actions.push(save_btn);
            actions.push(cancel_btn);
        }

        let card = Card::new()
            .with_title("Current Goal".to_string())
            .with_content(content)
            .with_actions(actions)
            .render(&fonts.heading);

        Some(
            Element::new(&fonts.body, ElementContent::Children(vec![card]))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(16.0),
                    right: Dimension::Pixels(16.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                }),
        )
    }

    fn render_current_suggestion(&mut self, fonts: &SidebarFonts) -> Option<Element> {
        let suggestion = self.current_suggestion.as_ref()?;

        // Clear previous more link bounds
        self.more_link_bounds = None;

        // Check if content would exceed 2 lines when wrapped
        const MAX_LINES: usize = 2;

        // Get approximate width available for text in the suggestion card
        // Sidebar: 16px padding each side = 32px
        // Card: 8px margin each side = 16px
        // Content container: 8px padding each side = 16px
        // Total: 32 + 16 + 16 = 64px
        let available_width = (self.width as f32) - 64.0;

        // Use our wrapping estimation to determine if we need truncation
        let estimated_lines =
            self.estimate_wrapped_lines(&suggestion.content, available_width, fonts);
        let needs_more_link = estimated_lines > MAX_LINES;

        let mut content_elements = vec![];

        if needs_more_link {
            // Truncate to fit within 2 lines using shared function
            let font_metrics = fonts.body.metrics();
            let avg_char_width =
                font_metrics.cell_height.get() as f32 * SUGGESTION_CHAR_WIDTH_MULTIPLIER;

            // Use shared truncation function
            let truncated_text = crate::termwindow::box_model::truncate_to_wrapped_lines(
                &suggestion.content,
                available_width,
                avg_char_width,
                MAX_LINES,
            );

            // Add ellipsis
            let display_text = format!("{}...", truncated_text);

            // Use plain text for truncated content
            content_elements.push(
                Element::new(&fonts.body, ElementContent::WrappedText(display_text))
                    .colors(ElementColors {
                        text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_height(Some(Dimension::Pixels(
                        2.0 * fonts.body.metrics().cell_height.get() as f32,
                    ))), // Fixed height for 2 lines
            );
        } else {
            // For short content, still use fixed height
            content_elements.push(
                Element::new(
                    &fonts.body,
                    ElementContent::WrappedText(suggestion.content.clone()),
                )
                .colors(ElementColors {
                    text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
                    ..Default::default()
                })
                .display(DisplayType::Block)
                .min_height(Some(Dimension::Pixels(
                    2.0 * fonts.body.metrics().cell_height.get() as f32,
                ))), // Fixed height for 2 lines
            );
        }

        let content_container =
            Element::new(&fonts.body, ElementContent::Children(content_elements))
                .display(DisplayType::Block)
                .padding(BoxDimension::new(Dimension::Pixels(8.0)));

        let mut actions = vec![];

        // Create a container for the action buttons
        let mut left_actions = vec![];
        let mut right_actions = vec![];

        if suggestion.has_action {
            let run_btn = Chip::new("▶ Run".to_string())
                .with_style(ChipStyle::Success)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::SuggestionRunButton)
                .render(&fonts.body);
            let dismiss_btn = Chip::new("✕ Dismiss".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::SuggestionDismissButton)
                .render(&fonts.body);

            left_actions.push(run_btn);
            left_actions.push(
                Element::new(&fonts.body, ElementContent::Text(" ".to_string()))
                    .min_width(Some(Dimension::Pixels(8.0))),
            );
            left_actions.push(dismiss_btn);
        }

        // Add "Show more" button on the right if needed
        if needs_more_link {
            let show_more_btn = Chip::new("Show more".to_string())
                .with_style(ChipStyle::Info)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::ShowMoreButton(
                    "current".to_string(),
                ))
                .render(&fonts.body);
            right_actions.push(show_more_btn);
        }

        // Create the action row with left and right alignment
        if !left_actions.is_empty() || !right_actions.is_empty() {
            // Use a flex-like approach with float for right alignment
            if !left_actions.is_empty() {
                for action in left_actions {
                    actions.push(action);
                }
            }

            if !right_actions.is_empty() {
                // Right-align the show more button using float
                for action in right_actions {
                    actions.push(action.float(Float::Right));
                }
            }
        }

        let card = Card::new()
            .with_title(suggestion.title.clone())
            .with_content(vec![content_container])
            .with_actions(actions)
            .render(&fonts.heading);

        Some(
            Element::new(&fonts.body, ElementContent::Children(vec![card]))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(16.0),
                    right: Dimension::Pixels(16.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                }),
        )
    }

    pub fn render_activity_item(
        &self,
        item: &ActivityItem,
        fonts: &SidebarFonts,
        item_index: usize,
        palette: &wezterm_term::color::ColorPalette,
    ) -> Element {
        match item {
            ActivityItem::Command {
                command,
                output,
                status,
                expanded,
                ..
            } => {
                let status_icon = match status {
                    CommandStatus::Running => "◐",
                    CommandStatus::Success => "✓",
                    CommandStatus::Failed(_) => "✕",
                };

                let status_color = match status {
                    CommandStatus::Running => LinearRgba::with_components(0.5, 0.7, 1.0, 1.0),
                    CommandStatus::Success => LinearRgba::with_components(0.4, 0.8, 0.4, 1.0),
                    CommandStatus::Failed(_) => LinearRgba::with_components(0.9, 0.4, 0.4, 1.0),
                };

                let mut content = vec![Element::new(
                    &fonts.body,
                    ElementContent::Text(format!("{} {}", status_icon, command)),
                )
                .colors(ElementColors {
                    text: status_color.into(),
                    ..Default::default()
                })];

                if *expanded && output.is_some() {
                    content.push(
                        Element::new(
                            &fonts.body,
                            ElementContent::Text(output.as_ref().unwrap().clone()),
                        )
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                            ..Default::default()
                        })
                        .padding(BoxDimension {
                            left: Dimension::Pixels(4.0),
                            top: Dimension::Pixels(4.0),
                            ..Default::default()
                        }),
                    );
                }

                Card::new().with_content(content).render(&fonts.body)
            }
            ActivityItem::Chat {
                message, is_user, ..
            } => {
                let bg_color = if *is_user {
                    LinearRgba::with_components(0.1, 0.3, 0.5, 0.3)
                } else {
                    LinearRgba::with_components(0.15, 0.15, 0.17, 1.0)
                };

                // Render message content with markdown if it's from AI
                let content = if *is_user {
                    Element::new(&fonts.body, ElementContent::WrappedText(message.clone())).colors(
                        ElementColors {
                            text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                            ..Default::default()
                        },
                    )
                } else {
                    // AI messages use markdown rendering with code font support
                    // Need to add width constraint for proper text wrapping
                    let sidebar_width = self.width as f32;
                    // Calculate available width accounting for all padding/margins:
                    // - Activity log container: no explicit padding
                    // - Chat message margin: 20px left or right = 20px
                    // - Chat message padding: 12px each side = 24px
                    // - Chat message border: 1px each side = 2px
                    // - Scrollbar space: ~12px
                    // Total: 20 + 24 + 2 + 12 = 58px
                    let content_width = sidebar_width - 58.0;
                    log::debug!(
                        "Rendering markdown in activity log: sidebar_width={}, content_width={}",
                        sidebar_width,
                        content_width
                    );

                    // Use registry if available for horizontal scrolling support
                    if let Some(ref registry) = self.code_block_registry {
                        MarkdownRenderer::render_with_fonts_registry_and_palette(
                            message,
                            fonts,
                            Some(content_width),
                            Arc::clone(registry),
                            &format!("activity_{}", item_index),
                            palette,
                        )
                    } else {
                        MarkdownRenderer::render_with_fonts(message, fonts, Some(content_width))
                    }
                    .max_width(Some(Dimension::Pixels(content_width)))
                };

                Element::new(&fonts.body, ElementContent::Children(vec![content]))
                    .display(DisplayType::Block)
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(12.0)))
                    .margin(BoxDimension {
                        left: if *is_user {
                            Dimension::Pixels(20.0)
                        } else {
                            Dimension::Pixels(0.0)
                        },
                        right: if *is_user {
                            Dimension::Pixels(0.0)
                        } else {
                            Dimension::Pixels(20.0)
                        },
                        bottom: Dimension::Pixels(8.0),
                        ..Default::default()
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.0)))
                    .colors(ElementColors {
                        border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 0.5)),
                        bg: bg_color.into(),
                        ..Default::default()
                    })
            }
            ActivityItem::Suggestion { title, content, .. } => {
                // Add width constraint for proper text wrapping
                let sidebar_width = self.width as f32;
                // Calculate available width for suggestion card content:
                // - Card margin: 8px each side = 16px
                // - Card padding: 12px each side = 24px
                // - Card border: 1px each side = 2px
                // - Scrollbar space: ~12px
                // Total: 16 + 24 + 2 + 12 = 54px
                let content_width = sidebar_width - 54.0;
                let markdown_content = if let Some(ref registry) = self.code_block_registry {
                    MarkdownRenderer::render_with_fonts_registry_and_palette(
                        content,
                        fonts,
                        Some(content_width),
                        Arc::clone(registry),
                        &format!("suggestion_{}", item_index),
                        palette,
                    )
                } else {
                    MarkdownRenderer::render_with_fonts(content, fonts, Some(content_width))
                };

                Card::new()
                    .with_title(format!("Past: {}", title))
                    .with_content(vec![
                        markdown_content.max_width(Some(Dimension::Pixels(content_width)))
                    ])
                    .render(&fonts.heading)
            }
            ActivityItem::Goal { text, .. } => {
                Element::new(&fonts.body, ElementContent::Text(format!("Goal: {}", text)))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.8, 0.8, 0.8, 1.0).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(8.0)))
            }
        }
    }

    /// Get filtered activity items based on current filter
    fn render_activity_log(
        &mut self,
        fonts: &SidebarFonts,
        available_height: f32,
        available_width: f32,
        palette: &wezterm_term::color::ColorPalette,
    ) -> Element {
        // Check if width has changed and invalidate cache if needed
        if let Some(last_width) = self.activity_log_last_width {
            let width_change = (last_width - available_width).abs();
            if width_change > WIDTH_CHANGE_THRESHOLD {
                log::info!(
                    "Significant width change from {} to {} (delta: {}), clearing height cache",
                    last_width,
                    available_width,
                    width_change
                );
                self.activity_log_height_cache.clear();
                self.height_trackers.clear();
            } else if width_change > 0.1 {
                log::trace!(
                    "Minor width change: {} -> {} (delta: {}), keeping cache",
                    last_width,
                    available_width,
                    width_change
                );
            }
        }
        self.activity_log_last_width = Some(available_width);

        // Filter items based on current filter
        let filtered_items: Vec<(usize, &ActivityItem)> = self
            .activity_log
            .iter()
            .enumerate()
            .filter(|(_, item)| match self.activity_filter {
                ActivityFilter::All => true,
                ActivityFilter::Commands => matches!(item, ActivityItem::Command { .. }),
                ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                ActivityFilter::Suggestions => matches!(item, ActivityItem::Suggestion { .. }),
            })
            .collect();

        let filtered_count = filtered_items.len();
        log::debug!(
            "Rendering activity log: {} total items, {} filtered items",
            self.activity_log.len(),
            filtered_count
        );

        // Get actual font metrics for accurate height calculations
        let font_metrics = fonts.body.metrics();
        let line_height = font_metrics.cell_height.get() as f32;

        // Calculate visible range based on scroll offset
        const BUFFER_ITEMS: usize = 3; // Render 3 items above and below viewport
        let viewport_start = self.activity_log_scroll_offset;
        let viewport_end = self.activity_log_scroll_offset + available_height;
        
        let mut first_visible: Option<usize> = None;
        let mut last_visible: Option<usize> = None;
        let mut current_y = 0.0;

        // Find which items are actually visible in the viewport
        for (idx, (orig_idx, item)) in filtered_items.iter().enumerate() {
            let item_id = match item {
                ActivityItem::Command { id, .. } => id.clone(),
                ActivityItem::Chat { id, .. } => id.clone(),
                ActivityItem::Suggestion { id, .. } => id.clone(),
                ActivityItem::Goal { id, .. } => id.clone(),
            };
            
            let item_height = self.get_activity_item_height(item, line_height, available_width);
            let item_start = current_y;
            let item_end = current_y + item_height;
            
            // Check if this item overlaps with the actual viewport
            let overlaps_viewport = item_end > viewport_start && item_start < viewport_end;
            
            // Enhanced debug logging for items near the viewport
            if idx < 3 || idx >= filtered_items.len() - 3 || 
               (item_end >= viewport_start - 200.0 && item_start <= viewport_end + 200.0) {
                log::debug!(
                    "[VSCROLL] Item {} (idx {}): y={:.0}-{:.0} (h={:.0}), viewport={:.0}-{:.0}, overlaps={}",
                    item_id, idx, item_start, item_end, item_height, viewport_start, viewport_end, overlaps_viewport
                );
            }
            
            // Special logging for items we expect but don't see
            if idx >= 5 && idx <= 10 {
                log::debug!(
                    "[VSCROLL] DEBUG Item {} (idx {}): start={:.0}, end={:.0}, height={:.0}",
                    item_id, idx, item_start, item_end, item_height
                );
            }
            
            // Track height measurement for tall items using scroll positions
            if item_height >= available_height || !self.height_trackers.get(&item_id).map(|t| t.seen_full_height).unwrap_or(false) {
                let tracker = self.height_trackers.entry(item_id.clone()).or_default();
                
                // Track when top edge enters viewport
                if overlaps_viewport && tracker.top_entered_at.is_none() {
                    tracker.top_entered_at = Some(self.activity_log_scroll_offset);
                    log::debug!(
                        "Item {} top entered viewport at scroll offset {}",
                        item_id, self.activity_log_scroll_offset
                    );
                }
                
                // Track when bottom edge becomes visible
                if tracker.top_entered_at.is_some() && item_end <= viewport_end {
                    if tracker.bottom_entered_at.is_none() {
                        tracker.bottom_entered_at = Some(self.activity_log_scroll_offset);
                        
                        // Calculate height from scroll distance
                        let top_offset = tracker.top_entered_at.unwrap();
                        let bottom_offset = tracker.bottom_entered_at.unwrap();
                        
                        // Scroll tracking is disabled - it was incorrectly adding viewport height
                        log::debug!(
                            "[VSCROLL] Scroll tracking DISABLED for {} - scroll_distance={:.0}px",
                            item_id, bottom_offset - top_offset
                        );
                    }
                }
            }
            
            // Log tall items for debugging
            if item_height > available_height {
                log::trace!(
                    "Tall item {} at idx {}: height={}, viewport={}, overlaps={}, pos={}..{}",
                    item_id,
                    idx,
                    item_height,
                    available_height,
                    overlaps_viewport,
                    item_start,
                    item_end
                );
            }
            
            // Check if any part of the item overlaps with the actual viewport
            if item_end > viewport_start && item_start < viewport_end {
                if first_visible.is_none() {
                    first_visible = Some(idx);
                }
                last_visible = Some(idx);
            }
            
            current_y = item_end; // Use item_end to be consistent
            
            // Note: We used to have an optimization here to stop scanning early,
            // but it was causing issues with calculating total height and finding all items.
            // We need to scan all items to get accurate total height.
        }
        
        // Log the scan result with more detail
        let total_content_height = current_y;
        log::debug!(
            "[VSCROLL] Scan complete: total_height={:.0}, viewport={:.0}-{:.0}, first_visible={:?}, last_visible={:?}",
            total_content_height, viewport_start, viewport_end, first_visible, last_visible
        );
        
        // DEBUG: Check if we can theoretically scroll to see all content
        let theoretical_max_scroll = (total_content_height - available_height).max(0.0);
        if self.activity_log_scroll_offset > theoretical_max_scroll - 10.0 {
            log::info!(
                "[VSCROLL] Near bottom: scroll={:.0}, max={:.0}, last_item_bottom={:.0}, viewport_bottom={:.0}",
                self.activity_log_scroll_offset, theoretical_max_scroll, total_content_height, 
                self.activity_log_scroll_offset + available_height
            );
        }
        
        // Apply consistent pixel-based buffer around the visible items
        
        let (start_idx, end_idx) = if let (Some(first), Some(last)) = (first_visible, last_visible) {
            // Find start index by going backwards from first_visible
            let mut start = first;
            let mut accumulated_before = 0.0;
            while start > 0 && accumulated_before < RENDER_MARGIN {
                start -= 1;
                if let Some((_, item)) = filtered_items.get(start) {
                    accumulated_before += self.get_activity_item_height(item, line_height, available_width);
                }
            }
            
            // Find end index by going forward from last_visible
            let mut end = last + 1;
            let mut accumulated_after = 0.0;
            while end < filtered_items.len() && accumulated_after < RENDER_MARGIN {
                if let Some((_, item)) = filtered_items.get(end) {
                    accumulated_after += self.get_activity_item_height(item, line_height, available_width);
                }
                end += 1;
            }
            
            log::debug!(
                "[VSCROLL] Pixel-based buffer: {}..{} (first_vis={}, last_vis={}, before={:.0}px, after={:.0}px)",
                start, end, first, last, accumulated_before, accumulated_after
            );
            
            (start, end)
        } else {
            // This should never happen if our heights are correct
            log::error!(
                "[VSCROLL] CRITICAL: No visible items found! viewport={:.0}-{:.0}, total_height={:.0}, item_count={}",
                viewport_start, viewport_end, current_y, filtered_items.len()
            );
            
            // Let's add some diagnostic info
            if filtered_items.is_empty() {
                log::error!("[VSCROLL] No items to display (empty list)");
                (0, 0)
            } else {
                // Log some sample heights to understand the issue
                log::error!("[VSCROLL] Sample item heights:");
                for i in 0..5.min(filtered_items.len()) {
                    if let Some((_, item)) = filtered_items.get(i) {
                        let h = self.get_activity_item_height(item, line_height, available_width);
                        log::error!("[VSCROLL]   Item {}: height={:.0}", i, h);
                    }
                }
                
                // Just show first few items rather than nothing
                let count = BUFFER_ITEMS.min(filtered_items.len());
                log::error!("[VSCROLL] Showing first {} items as fallback", count);
                (0, count)
            }
        };

        self.activity_log_visible_range = start_idx..end_idx;

        // DEBUG: Enhanced visible range logging
        log::info!(
            "[VSCROLL] Visible range: {:?} ({}..{}), First visible: {:?}, Last visible: {:?}",
            self.activity_log_visible_range, start_idx, end_idx, first_visible, last_visible
        );
        
        log::debug!(
            "[VSCROLL] Rendering {} items (indices {}..{}) of {} total, viewport: {:.0}-{:.0} pixels",
            end_idx - start_idx,
            start_idx,
            end_idx,
            filtered_items.len(),
            viewport_start,
            viewport_end
        );

        // Only render visible items
        let mut rendered_items: Vec<Element> = Vec::new();

        // Calculate Y offset for the first visible item
        // IMPORTANT: We need the offset to the first ACTUALLY VISIBLE item (first_visible),
        // not to start_idx which includes the buffer
        let mut y_offset_before_visible = 0.0;
        let actual_first_visible = first_visible.unwrap_or(0);
        for idx in 0..actual_first_visible {
            if let Some((orig_idx, item)) = filtered_items.get(idx) {
                y_offset_before_visible +=
                    self.get_activity_item_height(item, line_height, available_width);
            }
        }
        
        log::debug!(
            "[VSCROLL] y_offset_before_visible={:.0} (sum of {} items before first_visible={})",
            y_offset_before_visible, actual_first_visible, actual_first_visible
        );

        // Render visible items
        for idx in self.activity_log_visible_range.clone() {
            if let Some((orig_idx, item)) = filtered_items.get(idx) {
                let mut element = self.render_activity_item(item, fonts, *orig_idx, palette);

                // Attach cached height if available
                let item_id = match item {
                    ActivityItem::Command { id, .. } => id.clone(),
                    ActivityItem::Chat { id, .. } => id.clone(),
                    ActivityItem::Suggestion { id, .. } => id.clone(),
                    ActivityItem::Goal { id, .. } => id.clone(),
                };
                if let Some(height) = self.activity_log_height_cache.get(&item_id) {
                    element = element.with_computed_height(*height);
                }

                rendered_items.push(element);
            }
        }

        log::debug!(
            "[VSCROLL] Rendering {} visible items (of {} total), visible range: {:?}",
            rendered_items.len(),
            filtered_items.len(),
            self.activity_log_visible_range.clone()
        );

        // Calculate total content height
        let total_content_height =
            self.calculate_total_activity_log_height(&filtered_items, line_height, available_width);
        
        // Log height information
        log::debug!(
            "Total content height: {} pixels, scroll_offset: {}, max valid scroll: {}",
            total_content_height,
            self.activity_log_scroll_offset,
            (total_content_height - available_height).max(0.0)
        );
        
        // Before updating total height, calculate visual anchor if heights are changing
        let old_height = self.activity_log_scrollbar
            .as_ref()
            .map(|s| s.content_height)
            .unwrap_or(0.0);
            
        let height_changing = (old_height - total_content_height).abs() > 1.0;
        
        // TEMPORARILY DISABLED: Visual anchor system to fix scrolling jumps
        // if height_changing {
        //     // Calculate anchor before any changes
        //     self.visual_anchor = self.calculate_visual_anchor(
        //         &filtered_items,
        //         line_height,
        //         available_width,
        //         available_height
        //     );
        //     
        //     log::info!(
        //         "Total content height changing: {} -> {} (delta: {}, cache size: {})",
        //         old_height,
        //         total_content_height,
        //         total_content_height - old_height,
        //         self.activity_log_height_cache.len()
        //     );
        // }

        // DEBUG: Log comprehensive state information
        log::info!(
            "[VSCROLL] Total items: {}, Filtered: {}, Total height: {:.0}px, Scroll: {:.0}px, Viewport: {:.0}px, Max scroll: {:.0}px",
            self.activity_log.len(), 
            filtered_items.len(), 
            total_content_height, 
            self.activity_log_scroll_offset, 
            available_height, 
            (total_content_height - available_height).max(0.0)
        );
        
        // Debug: Show what's at the end of the list
        if let Some((idx, last_item)) = filtered_items.last() {
            let last_height = self.get_activity_item_height(last_item, line_height, available_width);
            let last_id = match last_item {
                ActivityItem::Command { id, .. } => id,
                ActivityItem::Chat { id, .. } => id,
                ActivityItem::Suggestion { id, .. } => id,
                ActivityItem::Goal { id, .. } => id,
            };
            let is_cached = self.activity_log_height_cache.contains_key(last_id);
            log::debug!(
                "[VSCROLL] Last item: {} (idx={}, height={:.0}px, cached={}), can_reach_end={}",
                last_id, idx, last_height, is_cached,
                self.activity_log_scroll_offset + available_height >= total_content_height - 10.0
            );
            
            // Check last few items to see if they have cached heights
            let last_5_uncached = filtered_items.iter().rev().take(5)
                .filter(|(_, item)| {
                    let id = match item {
                        ActivityItem::Command { id, .. } => id,
                        ActivityItem::Chat { id, .. } => id,
                        ActivityItem::Suggestion { id, .. } => id,
                        ActivityItem::Goal { id, .. } => id,
                    };
                    !self.activity_log_height_cache.contains_key(id)
                })
                .count();
            if last_5_uncached > 0 {
                log::debug!(
                    "[VSCROLL] {} of last 5 items are using estimated heights (never been visible)",
                    last_5_uncached
                );
                
                // Show which specific items are uncached
                let uncached_info: Vec<String> = filtered_items.iter().rev().take(5)
                    .filter_map(|(idx, item)| {
                        let id = match item {
                            ActivityItem::Command { id, .. } => id,
                            ActivityItem::Chat { id, .. } => id,
                            ActivityItem::Suggestion { id, .. } => id,
                            ActivityItem::Goal { id, .. } => id,
                        };
                        if !self.activity_log_height_cache.contains_key(id) {
                            Some(format!("{} (idx={})", id, idx))
                        } else {
                            None
                        }
                    })
                    .collect();
                log::debug!("[VSCROLL] Uncached items: {:?}", uncached_info);
            }
        }

        // Ensure scroll offset is within valid bounds
        let max_valid_scroll = (total_content_height - available_height).max(0.0);
        if self.activity_log_scroll_offset > max_valid_scroll {
            log::warn!(
                "Scroll offset {} exceeds max valid scroll {}, clamping",
                self.activity_log_scroll_offset,
                max_valid_scroll
            );
            self.activity_log_scroll_offset = max_valid_scroll;
        }
        
        // Update scrollbar state
        let scrollbar_info = ScrollbarInfo {
            should_show: total_content_height > available_height,
            thumb_position: if total_content_height > available_height {
                self.activity_log_scroll_offset / (total_content_height - available_height)
            } else {
                0.0
            },
            thumb_size: (available_height / total_content_height).min(1.0).max(0.1),
            content_height: total_content_height,
            viewport_height: available_height,
            scroll_offset: self.activity_log_scroll_offset,
            total_items: filtered_items.len(),
            viewport_items: self.activity_log_visible_range.len(),
        };

        self.activity_log_scrollbar = Some(scrollbar_info.clone());

        // Update scrollbar renderer
        if scrollbar_info.should_show {
            match &mut self.activity_log_scrollbar_renderer {
                Some(renderer) => {
                    renderer.update(
                        total_content_height,
                        available_height,
                        self.activity_log_scroll_offset,
                    );
                }
                None => {
                    self.activity_log_scrollbar_renderer = Some(ScrollbarRenderer::new_vertical(
                        total_content_height,
                        available_height,
                        self.activity_log_scroll_offset,
                        20.0, // min thumb size
                    ));
                }
            }
        } else {
            self.activity_log_scrollbar_renderer = None;
        }

        // Create scrollable container with only visible elements
        let margin_top = -self.activity_log_scroll_offset + y_offset_before_visible;
        log::debug!(
            "[VSCROLL] Content positioning: scroll_offset={:.0}, y_offset_before_visible={:.0}, margin_top={:.0}",
            self.activity_log_scroll_offset,
            y_offset_before_visible,
            margin_top
        );
        
        let content_area = Element::new(&fonts.body, ElementContent::Children(rendered_items))
            .display(DisplayType::Block)
            .margin(BoxDimension {
                top: Dimension::Pixels(margin_top),
                ..Default::default()
            });

        // Create viewport container with fixed height and clipping
        let viewport = Element::new(&fonts.body, ElementContent::Children(vec![content_area]))
            .display(DisplayType::Block)
            .min_height(Some(Dimension::Pixels(available_height)));
            
        // Log diagnostics when content might be invisible
        if margin_top < -5000.0 || self.activity_log_scroll_offset > total_content_height {
            log::warn!(
                "Potential visibility issue: margin_top={}, scroll_offset={}, total_height={}, viewport_height={}",
                margin_top,
                self.activity_log_scroll_offset,
                total_content_height,
                available_height
            );
        }
        
        viewport
    }

    fn render_chat_input(&self, fonts: &SidebarFonts) -> Element {
        let input_field = self.chat_input.render(&fonts.body);

        let send_button = Chip::new("Send".to_string())
            .with_style(ChipStyle::Primary)
            .with_size(ChipSize::Medium)
            .clickable(true)
            .render(&fonts.body);

        Element::new(
            &fonts.body,
            ElementContent::Children(vec![input_field, send_button]),
        )
        .display(DisplayType::Block)
        .padding(BoxDimension {
            left: Dimension::Pixels(16.0),
            right: Dimension::Pixels(16.0),
            top: Dimension::Pixels(8.0),
            bottom: Dimension::Pixels(16.0),
        })
    }

    /// Render the activity log separately for layered rendering
    pub fn render_activity_log_content(
        &mut self,
        fonts: &SidebarFonts,
        window_height: f32,
        palette: &wezterm_term::color::ColorPalette,
    ) -> Element {
        // Performance Note: This method is called on every paint frame, causing markdown
        // to be re-rendered 60+ times per second. Future optimizations could include:
        // 1. Caching rendered Elements (requires making Element Send+Sync)
        // 2. Only rendering visible items (viewport culling)
        // 3. Detecting when content/theme/size hasn't changed
        // 4. Moving markdown parsing to a background thread
        // For now, we rely on the efficiency of the markdown parser and renderer.
        // Get the dynamic bounds for the activity log
        let bounds = self
            .get_activity_log_bounds(window_height)
            .unwrap_or_else(|| {
                euclid::rect(16.0, 200.0, self.width as f32 - 32.0, window_height - 320.0)
            });

        // The activity log height is the bounds height
        let available_for_log = bounds.size.height;
        let available_width = bounds.size.width;

        // Render the activity log content
        let activity_log =
            self.render_activity_log(fonts, available_for_log, available_width, palette);

        // Wrap in a container with background color
        let container = Element::new(&fonts.body, ElementContent::Children(vec![activity_log]))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.03, 0.03, 0.035, 1.0).into(), // Slightly lighter than sidebar
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(bounds.size.width)))
            .min_height(Some(Dimension::Pixels(bounds.size.height)));

        container
    }

    pub fn render_content(&mut self, fonts: &SidebarFonts, window_height: f32) -> Element {
        let mut children = vec![];

        // Fixed height elements at top
        // Header
        children.push(self.render_header(fonts));

        // Status chip
        children.push(self.render_status_chip(fonts));

        // Filter chips
        children.push(self.render_filter_chips(fonts));

        // Current goal card
        if let Some(goal_element) = self.render_current_goal(fonts) {
            children.push(goal_element);
        }

        // Current suggestion card
        if let Some(suggestion_element) = self.render_current_suggestion(fonts) {
            children.push(suggestion_element);
        }

        // Use the already calculated bounds
        let bounds = self
            .get_activity_log_bounds(window_height)
            .unwrap_or_else(|| {
                euclid::rect(16.0, 200.0, self.width as f32 - 32.0, window_height - 320.0)
            });

        // The spacer should fill the remaining space in the window
        // Total height = sum of all components
        // We already have: header + status + filters + goal + suggestion = bounds.origin.y
        // We need: spacer + chat_input = window_height - bounds.origin.y
        // So spacer = window_height - bounds.origin.y - chat_input_height
        let chat_input_height = 74.0;
        let spacer_height = (window_height - bounds.origin.y - chat_input_height).max(0.0);

        log::debug!(
            "Sidebar layout: window_height={}, content_above_log={}, spacer_height={}, chat_height={}",
            window_height, bounds.origin.y, spacer_height, chat_input_height
        );

        // Skip the activity log here - it will be rendered separately at a different z-index
        // Add a transparent spacer to maintain layout
        children.push(
            Element::new(&fonts.body, ElementContent::Text(String::new()))
                .display(DisplayType::Block)
                .min_height(Some(Dimension::Pixels(spacer_height)))
                // Completely transparent - no background
                .colors(ElementColors {
                    bg: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0).into(),
                    ..Default::default()
                }),
        );

        // Fixed height chat input at bottom
        children.push(self.render_chat_input(fonts));

        // Container - transparent so the hole works
        Element::new(&fonts.heading, ElementContent::Children(children))
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(self.width as f32)))
            .min_height(Some(Dimension::Pixels(window_height)))
    }

    pub fn handle_filter_click(&mut self, filter: ActivityFilter) {
        if self.activity_filter != filter {
            // Clear height trackers when filter changes as item indices will change
            self.height_trackers.clear();
            // Keep height cache as the heights are still valid for the same items
            log::debug!("Filter changed to {:?}, cleared height trackers", filter);
        }
        self.activity_filter = filter;
    }

    pub fn handle_goal_confirm(&mut self) {
        if let Some(goal) = &mut self.current_goal {
            goal.is_confirmed = true;
        }
    }

    pub fn handle_goal_edit_toggle(&mut self) {
        if let Some(goal) = &mut self.current_goal {
            goal.is_editing = !goal.is_editing;
            if goal.is_editing {
                goal.edit_text = goal.text.clone();
            }
        }
    }

    /// Estimate how many lines text will wrap to given available width
    fn estimate_wrapped_lines(
        &self,
        text: &str,
        available_width: f32,
        fonts: &SidebarFonts,
    ) -> usize {
        // Get font metrics for accurate estimation
        let font_metrics = fonts.body.metrics();
        let avg_char_width =
            font_metrics.cell_height.get() as f32 * SUGGESTION_CHAR_WIDTH_MULTIPLIER;

        // Use the shared utility function (integer version)
        crate::termwindow::box_model::estimate_wrapped_line_count(
            text,
            available_width,
            avg_char_width,
        )
    }

    pub fn handle_goal_save(&mut self) {
        if let Some(goal) = &mut self.current_goal {
            goal.text = goal.edit_text.clone();
            goal.is_editing = false;
            goal.is_ai_inferred = false;
            goal.is_confirmed = true;
        }
    }

    pub fn handle_suggestion_run(&mut self) {
        // Would trigger command execution
        println!("Running suggestion command...");
    }

    pub fn handle_suggestion_dismiss(&mut self) {
        self.current_suggestion = None;
    }

    pub fn handle_chat_input(&mut self, c: char) {
        self.chat_input.insert_char(c);
    }

    pub fn handle_chat_send(&mut self) {
        let text = self.chat_input.get_text();
        if !text.trim().is_empty() {
            self.activity_log.push(ActivityItem::Chat {
                id: format!("chat_{}", self.activity_log.len()),
                message: text,
                is_user: true,
                timestamp: SystemTime::now(),
            });
            self.chat_input.clear();
            // Clear code block registry since content has changed
            self.clear_code_block_registry();
        }
    }

    /// Set the scrollbar bounds for hit testing
    pub fn set_scrollbar_bounds(&mut self, bounds: euclid::Rect<f32, window::PixelUnit>) {
        log::debug!(
            "Setting scrollbar bounds: origin=({}, {}), size=({}, {})",
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.width,
            bounds.size.height
        );
        self.activity_log_scrollbar_bounds = Some(bounds);
    }

    /// Get the bounds of the activity log viewport for clipping
    pub fn get_activity_log_bounds(
        &self,
        window_height: f32,
    ) -> Option<euclid::Rect<f32, window::PixelUnit>> {
        // Calculate dynamic positions based on ACTUAL rendered heights:
        // Header: 58px
        let mut top = 58.0;

        // Status chip
        top += 52.0;

        // Filter chips
        top += 55.0;

        // Add goal card height if present
        if self.current_goal.is_some() {
            top += 201.0;
        }

        // Add suggestion card height if present
        if self.current_suggestion.is_some() {
            // Setting to match visual observation
            top += 201.0;
        }

        // Add padding between last card and activity log for visual separation
        top += 10.0; // Increased for better visual separation

        // Bottom calculation
        // Add small margin to ensure it doesn't touch the bottom
        let bottom = window_height - 90.0;
        let left = 16.0; // Padding
        let right = self.width as f32 - 16.0; // Right padding for scrollbar

        log::debug!(
            "Activity log bounds: top={}, bottom={}, left={}, right={}, height={}",
            top,
            bottom,
            left,
            right,
            bottom - top
        );

        Some(euclid::rect(left, top, right - left, bottom - top))
    }

    /// Check if a mouse event is within the scrollbar bounds
    fn is_scrollbar_event(&self, event: &MouseEvent) -> bool {
        if let Some(bounds) = &self.activity_log_scrollbar_bounds {
            let point = euclid::point2(event.coords.x as f32, event.coords.y as f32);
            let contains = bounds.contains(point);
            log::debug!(
                "Checking scrollbar bounds: point=({}, {}), bounds=({}, {}, {}, {}), contains={}",
                point.x,
                point.y,
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width,
                bounds.size.height,
                contains
            );
            contains
        } else {
            log::debug!("No scrollbar bounds set");
            false
        }
    }

    /// Update sidebar position for mouse event handling
    pub fn update_sidebar_position(&mut self, sidebar_x: f32) {
        // Store sidebar position for mouse event handling
        self.sidebar_x_position = sidebar_x;
    }

    /// Check which filter chip was clicked based on coordinates
    fn get_clicked_filter(&self, event: &MouseEvent, sidebar_x: f32) -> Option<ActivityFilter> {
        // Check if click is in the filter chip area (approximate Y range)
        // Header: 58px, Status chip: 52px = 110px top
        // Filter chips height: ~55px, so range is 110-165
        let y = event.coords.y as f32;
        if y < 110.0 || y > 165.0 {
            log::debug!("Click Y {} outside filter range 110-165", y);
            return None;
        }

        // Convert window X coordinate to sidebar-relative X
        let relative_x = event.coords.x as f32 - sidebar_x;

        // The chips are laid out starting at x=16 within the sidebar
        // Approximate widths: All(35), Commands(75), Chat(40), Suggestions(85)
        // With 8px spacing between chips
        let base_x = 16.0;
        if relative_x < base_x {
            return None;
        }

        let x = relative_x - base_x;
        log::debug!("Filter chip click: relative_x={}, x={}", relative_x, x);

        // Updated measurements based on actual chip sizes
        // Small chips have ~6px padding each side + text width
        if x < 47.0 {
            // "All" chip (~35px text + 12px padding)
            Some(ActivityFilter::All)
        } else if x < 142.0 {
            // 47 + 8 + 87 ("Commands" ~75px + 12px)
            Some(ActivityFilter::Commands)
        } else if x < 202.0 {
            // 142 + 8 + 52 ("Chat" ~40px + 12px)
            Some(ActivityFilter::Chat)
        } else if x < 299.0 {
            // 202 + 8 + 97 ("Suggestions" ~85px + 12px)
            Some(ActivityFilter::Suggestions)
        } else {
            None
        }
    }

    pub fn show_suggestion_modal(&mut self, suggestion: CurrentSuggestion) {
        let modal = Modal {
            id: "suggestion_modal".to_string(),
            size: ModalSize::FillSidebar,
            content: Box::new(SuggestionModal::new(suggestion)),
            animation_state: crate::sidebar::components::modal::ModalAnimationState::Opening,
            close_on_click_outside: true,
            close_on_escape: true,
            position: None,
        };
        self.modal_manager.show(modal);
    }

    pub fn close_modal(&mut self) {
        self.modal_manager.close();
    }

    pub fn get_current_suggestion(&self) -> Option<&CurrentSuggestion> {
        self.current_suggestion.as_ref()
    }

    /// Clear code block registry when content changes completely
    pub fn clear_code_block_registry(&mut self) {
        if let Some(ref registry) = self.code_block_registry {
            if let Ok(mut reg) = registry.lock() {
                reg.clear();
            }
        }
    }

    pub fn render_modals(&mut self, fonts: &SidebarFonts, window_height: f32) -> Vec<Element> {
        // Get sidebar bounds
        let sidebar_bounds = euclid::rect(
            self.sidebar_x_position,
            0.0,
            self.width as f32,
            window_height,
        );

        // Get window bounds (we'll need to pass this from the parent)
        // For now, use a reasonable default
        let window_bounds = euclid::rect(
            0.0,
            0.0,
            self.sidebar_x_position + self.width as f32 + 100.0, // Approximate window width
            window_height,
        );

        self.modal_manager.render(
            sidebar_bounds,
            window_bounds,
            fonts,
            self.code_block_registry.clone(),
        )
    }
}

impl Sidebar for AiSidebar {
    fn render(&mut self, fonts: &SidebarFonts, window_height: f32) -> Element {
        self.render_content(fonts, window_height)
    }

    fn get_scrollbars(&self) -> super::SidebarScrollbars {
        super::SidebarScrollbars {
            activity_log: self.activity_log_scrollbar.clone(),
        }
    }

    fn get_width(&self) -> u16 {
        self.width
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    fn get_position(&self) -> SidebarPosition {
        SidebarPosition::Right
    }

    fn set_width(&mut self, width: u16) {
        self.width = width;
    }

    fn handle_mouse_event(&mut self, event: &MouseEvent) -> Result<bool> {
        log::debug!(
            "AI sidebar handle_mouse_event: {:?} at ({}, {})",
            event.kind,
            event.coords.x,
            event.coords.y
        );

        // Handle modal events first - if modal is active, it captures ALL events
        if self.modal_manager.is_active() {
            let sidebar_bounds = euclid::rect(
                self.sidebar_x_position,
                0.0,
                self.width as f32,
                1000.0, // Use a reasonable default height
            );
            // Always let modal handle the event when it's active
            let handled = self.modal_manager.handle_mouse_event(event, sidebar_bounds);
            // For scroll wheel events, always return true when modal is active to prevent
            // the activity log from scrolling behind the modal
            if matches!(event.kind, WMEK::VertWheel(_)) {
                return Ok(true);
            }
            if handled {
                return Ok(true);
            }
        }

        // Code block horizontal scrolling has been removed - using line wrapping instead

        // Show more button is now handled via UIItemType

        // Log current bounds for debugging
        if let WMEK::Press(MousePress::Left) = &event.kind {
            if let Some(bounds) = &self.activity_log_scrollbar_bounds {
                log::debug!(
                    "Scrollbar bounds: x={}, y={}, w={}, h={}",
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.width,
                    bounds.size.height
                );
            }
            log::debug!("Filter chip bounds:");
            for (filter, bounds) in &self.filter_chip_bounds {
                log::debug!(
                    "  {:?}: x={}, y={}, w={}, h={}",
                    filter,
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.width,
                    bounds.size.height
                );
            }
        }

        // Handle scroll wheel events
        if let WMEK::VertWheel(amount) = &event.kind {
            log::debug!(
                "Scroll wheel event: amount={}, has_renderer={}",
                amount,
                self.activity_log_scrollbar_renderer.is_some()
            );

            // Check if we have a scrollbar renderer to get scroll metrics
            if let Some(renderer) = &self.activity_log_scrollbar_renderer {
                let scroll_speed = 20.0; // Pixels per scroll step (roughly 1 line)
                let scroll_amount = scroll_speed * (*amount as f32).abs(); // 1 line per scroll

                let old_offset = self.activity_log_scroll_offset;
                let new_offset = if *amount > 0 {
                    // Scroll up
                    (self.activity_log_scroll_offset - scroll_amount).max(0.0)
                } else {
                    // Scroll down
                    self.activity_log_scroll_offset + scroll_amount
                };

                // Constrain to valid range using actual content metrics
                let max_scroll = (renderer.total_size() - renderer.viewport_size()).max(0.0);
                self.activity_log_scroll_offset = new_offset.clamp(0.0, max_scroll);

                let actually_scrolled = (self.activity_log_scroll_offset - old_offset).abs() > 0.1;
                log::debug!(
                    "Scroll wheel: old_offset={}, new_offset={}, max_scroll={}, amount={}, scroll_amount={}, actually_moved={}",
                    old_offset, self.activity_log_scroll_offset, max_scroll, amount, scroll_amount, actually_scrolled
                );
                
                // Return true even if we didn't move to consume the event
                return Ok(true);
            } else {
                log::debug!("No scrollbar renderer for scroll wheel");
            }
        }

        // Check if we need to handle scrollbar events
        // Always process mouse events if the scrollbar is currently being dragged,
        // even if the mouse is outside the scrollbar bounds
        let should_handle_scrollbar = if let Some(renderer) = &self.activity_log_scrollbar_renderer
        {
            renderer.state().is_dragging || self.is_scrollbar_event(event)
        } else {
            false
        };

        if should_handle_scrollbar {
            if let Some(renderer) = &mut self.activity_log_scrollbar_renderer {
                if let Some(bounds) = &self.activity_log_scrollbar_bounds {
                    // Handle the mouse event with the scrollbar renderer
                    if let Some(new_scroll_offset) = renderer.handle_mouse_event(event, *bounds) {
                        // Update scroll position
                        self.activity_log_scroll_offset = new_scroll_offset.max(0.0);
                        
                        // Clear visual anchor when user interacts with scrollbar
                        self.visual_anchor = None;
                        
                        log::debug!(
                            "Scrollbar updated scroll offset to: {}",
                            self.activity_log_scroll_offset
                        );
                        return Ok(true);
                    }
                    return Ok(renderer.state().is_dragging);
                }
            }
        }

        // Filter chip clicks are now handled through UIItemType
        // Just return false to let the UIItem system handle it
        Ok(false)
    }

    fn handle_key_event(&mut self, key: &KeyCode) -> Result<bool> {
        // Handle modal keyboard events first
        if self.modal_manager.is_active() {
            if self
                .modal_manager
                .handle_key_event(*key, KeyModifiers::empty())
            {
                return Ok(true);
            }
        }

        // Code block keyboard navigation removed - using line wrapping instead

        // Focus the chat input for now (in future, handle focus states)
        self.chat_input.focused = true;

        match key {
            KeyCode::Char('\n') | KeyCode::Char('\r') => {
                // Newline characters - insert newline
                self.chat_input.insert_newline();
                Ok(true)
            }
            KeyCode::Char(c) => {
                // All other characters
                self.chat_input.insert_char(*c);
                Ok(true)
            }
            KeyCode::Enter => {
                // Enter to send
                self.handle_chat_send();
                Ok(true)
            }
            KeyCode::Backspace => {
                self.chat_input.backspace();
                Ok(true)
            }
            KeyCode::Delete => {
                self.chat_input.delete();
                Ok(true)
            }
            KeyCode::UpArrow => {
                self.chat_input.move_up();
                Ok(true)
            }
            KeyCode::DownArrow => {
                self.chat_input.move_down();
                Ok(true)
            }
            KeyCode::LeftArrow => {
                self.chat_input.move_left();
                Ok(true)
            }
            KeyCode::RightArrow => {
                self.chat_input.move_right();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl AiSidebar {
    /// Check if any animations need frame updates
    pub fn needs_animation_frame(&self) -> bool {
        // For now, activity log scrollbar doesn't have animations since auto_hide is false
        // Only check modal animations
        false
    }

    /// Get a mutable reference to the modal manager for animation updates
    pub fn modal_manager_mut(&mut self) -> &mut ModalManager {
        &mut self.modal_manager
    }

    /// Get the height of an activity item (cached or estimated)
    fn get_activity_item_height(
        &self,
        item: &ActivityItem,
        line_height: f32,
        available_width: f32,
    ) -> f32 {
        let id = match item {
            ActivityItem::Command { id, .. } => id.clone(),
            ActivityItem::Chat { id, .. } => id.clone(),
            ActivityItem::Suggestion { id, .. } => id.clone(),
            ActivityItem::Goal { id, .. } => id.clone(),
        };
        
        if let Some(cached_height) = self.activity_log_height_cache.get(&id) {
            let estimated = estimate_activity_item_height(item, line_height, available_width);
            if (cached_height - estimated).abs() > 100.0 {
                log::warn!(
                    "[VSCROLL] Large height difference for {}: cached={:.0} vs estimated={:.0} (delta={:.0})",
                    id, cached_height, estimated, cached_height - estimated
                );
            }
            *cached_height
        } else {
            let estimated = estimate_activity_item_height(item, line_height, available_width);
            log::trace!("Height cache MISS for {}: estimated {} pixels", id, estimated);
            estimated
        }
    }

    /// Calculate total content height
    fn calculate_total_activity_log_height(
        &self,
        filtered_items: &[(usize, &ActivityItem)],
        line_height: f32,
        available_width: f32,
    ) -> f32 {
        filtered_items
            .iter()
            .map(|(_, item)| self.get_activity_item_height(item, line_height, available_width))
            .sum::<f32>()
        // Removed +20px hack - virtual scrolling with accurate height caching handles this correctly
    }

    /// Update height cache from rendered ComputedElement
    /// This extracts the actual rendered heights from the computed element tree
    pub fn update_activity_log_height_cache(
        &mut self,
        activity_log_computed: &crate::termwindow::box_model::ComputedElement,
        viewport_height: f32,
    ) {
        use crate::termwindow::box_model::ComputedElementContent;
        
        // Remember if we were at the bottom before updating
        let was_at_bottom = if let Some(scrollbar) = &self.activity_log_scrollbar {
            let max_scroll = (scrollbar.content_height - scrollbar.viewport_height).max(0.0);
            let at_bottom = max_scroll > 0.0 && self.activity_log_scroll_offset >= max_scroll - 1.0;
            if at_bottom {
                log::debug!(
                    "[VSCROLL] was_at_bottom=true (scroll={:.0}, max={:.0}, content={:.0})",
                    self.activity_log_scroll_offset, max_scroll, scrollbar.content_height
                );
            }
            at_bottom
        } else {
            false
        };
        
        // Remember the old total height
        let old_total_height = self.activity_log_scrollbar
            .as_ref()
            .map(|s| s.content_height)
            .unwrap_or(0.0);

        // The activity log computed element structure is:
        // - Root container (with margin for scrolling)
        //   - Content area (with children for each visible item)

        // Track if we need sticky bottom (currently disabled)
        let sticky_bottom_needed = was_at_bottom;
        
        // First, try to find the content area with the visible items
        // The structure is: root → viewport → content_area → [items]
        if let ComputedElementContent::Children(ref root_children) = activity_log_computed.content {
            log::debug!(
                "[VSCROLL] Root element has {} children, bounds height: {:.0}",
                root_children.len(),
                activity_log_computed.bounds.height()
            );
            
            if let Some(viewport) = root_children.first() {
                log::debug!(
                    "[VSCROLL] Viewport bounds: origin=({:.0}, {:.0}), size=({:.0} x {:.0})",
                    viewport.bounds.origin.x,
                    viewport.bounds.origin.y,
                    viewport.bounds.size.width,
                    viewport.bounds.size.height
                );
                
                if let ComputedElementContent::Children(ref viewport_children) = viewport.content {
                    log::debug!(
                        "[VSCROLL] Viewport has {} children",
                        viewport_children.len()
                    );
                    
                    if let Some(content_area) = viewport_children.first() {
                        log::debug!(
                            "[VSCROLL] Content area bounds: origin=({:.0}, {:.0}), size=({:.0} x {:.0})",
                            content_area.bounds.origin.x,
                            content_area.bounds.origin.y,
                            content_area.bounds.size.width,
                            content_area.bounds.size.height
                        );
                        
                        if let ComputedElementContent::Children(ref item_elements) = content_area.content {
                            // Now we have the individual item elements
                            // We need to map these back to the visible items
                            
                            log::debug!(
                                "[VSCROLL] Found {} item elements in content area (visible range has {} items)",
                                item_elements.len(),
                                self.activity_log_visible_range.clone().count()
                            );

                    // Get the filtered items to match against
                    let filtered_items: Vec<(usize, &ActivityItem)> = self
                        .activity_log
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| match self.activity_filter {
                            ActivityFilter::All => true,
                            ActivityFilter::Commands => {
                                matches!(item, ActivityItem::Command { .. })
                            }
                            ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                            ActivityFilter::Suggestions => {
                                matches!(item, ActivityItem::Suggestion { .. })
                            }
                        })
                        .collect();

                    // Debug: Check if we have the expected number of elements
                    if item_elements.len() != self.activity_log_visible_range.clone().count() {
                        log::warn!(
                            "[VSCROLL] Mismatch: expected {} item elements, found {}",
                            self.activity_log_visible_range.clone().count(),
                            item_elements.len()
                        );
                    }
                    
                    // For each computed element in the visible range
                    for (relative_idx, computed_item) in item_elements.iter().enumerate() {
                        // Map relative index to actual index in visible range
                        if let Some(visible_idx) =
                            self.activity_log_visible_range.clone().nth(relative_idx)
                        {
                            if let Some((_, item)) = filtered_items.get(visible_idx) {
                                // Get the item ID
                                let item_id = match item {
                                    ActivityItem::Command { id, .. } => id.clone(),
                                    ActivityItem::Chat { id, .. } => id.clone(),
                                    ActivityItem::Suggestion { id, .. } => id.clone(),
                                    ActivityItem::Goal { id, .. } => id.clone(),
                                };

                                // Extract the rendered height
                                // Use border_rect height which includes the full rendered height with padding/borders
                                let rendered_height = computed_item.border_rect.size.height;
                                
                                // DEBUG: Log height comparison
                                log::debug!(
                                    "[VSCROLL] HEIGHT DEBUG {}: calculated={:.0}, border_rect.h={:.0}, content_rect.h={:.0}, y_pos={:.0}",
                                    item_id,
                                    rendered_height,
                                    computed_item.border_rect.size.height,
                                    computed_item.content_rect.size.height,
                                    computed_item.content_rect.origin.y
                                );
                                
                                // Special logging for tall items
                                if item_id.contains("chat2") || computed_item.border_rect.size.height > 1000.0 {
                                    log::info!(
                                        "[VSCROLL] TALL ITEM {}: border_rect.h={:.0} (vs viewport={:.0}), clipped={}",
                                        item_id,
                                        computed_item.border_rect.size.height,
                                        viewport_height,
                                        computed_item.border_rect.size.height > viewport_height
                                    );
                                }
                                
                                // DEBUG: Log detailed element information
                                log::debug!(
                                    "[VSCROLL] Item {} element #{} debug:",
                                    item_id, relative_idx
                                );
                                log::debug!(
                                    "  CALCULATED height: {:.0}px (position-based)",
                                    rendered_height
                                );
                                log::debug!(
                                    "  content_rect: origin=({:.0}, {:.0}) size=({:.0} x {:.0})",
                                    computed_item.content_rect.origin.x,
                                    computed_item.content_rect.origin.y,
                                    computed_item.content_rect.size.width,
                                    computed_item.content_rect.size.height
                                );
                                log::debug!(
                                    "  bounds: origin=({:.0}, {:.0}) size=({:.0} x {:.0})",
                                    computed_item.bounds.origin.x,
                                    computed_item.bounds.origin.y,
                                    computed_item.bounds.size.width,
                                    computed_item.bounds.size.height
                                );
                                
                                // Log the margin/padding info if the height seems wrong
                                if computed_item.content_rect.size.height > 2000.0 {
                                    let padding_height = computed_item.padding.height() - computed_item.content_rect.height();
                                    let border_height = computed_item.border_rect.height() - computed_item.padding.height();
                                    let margin_height = computed_item.bounds.height() - computed_item.border_rect.height();
                                    
                                    log::debug!(
                                        "  HEIGHT BREAKDOWN: content={:.0}, +padding={:.0}, +border={:.0}, +margin={:.0}",
                                        computed_item.content_rect.size.height,
                                        padding_height,
                                        border_height,
                                        margin_height
                                    );
                                }
                                log::debug!(
                                    "  border_rect: origin=({:.0}, {:.0}) size=({:.0} x {:.0})",
                                    computed_item.border_rect.origin.x,
                                    computed_item.border_rect.origin.y,
                                    computed_item.border_rect.size.width,
                                    computed_item.border_rect.size.height
                                );
                                
                                // Check what type of content this element has
                                match &computed_item.content {
                                    ComputedElementContent::Text(_) => {
                                        log::debug!("  content type: Text");
                                    }
                                    ComputedElementContent::Children(children) => {
                                        log::debug!("  content type: Children (count: {})", children.len());
                                        
                                        // For elements with children, try to calculate height from children
                                        if !children.is_empty() {
                                            let first_child_y = children.first().unwrap().content_rect.origin.y;
                                            let last_child = children.last().unwrap();
                                            let last_child_bottom = last_child.content_rect.origin.y + last_child.content_rect.size.height;
                                            let calculated_height = last_child_bottom - first_child_y;
                                            
                                            log::debug!(
                                                "  calculated height from children: {:.0} (first_y: {:.0}, last_bottom: {:.0})",
                                                calculated_height, first_child_y, last_child_bottom
                                            );
                                        }
                                    }
                                    _ => {
                                        log::debug!("  content type: Other");
                                    }
                                }
                                
                                // Calculate this item's position in the viewport
                                // The computed element's bounds.origin.y tells us where it is positioned
                                // relative to the viewport (after scroll transform is applied)
                                
                                let item_top = computed_item.content_rect.origin.y;
                                let item_bottom = item_top + rendered_height;
                                
                                // Check if this item is fully visible (no clipping)
                                let is_fully_visible = item_top >= 0.0 && item_bottom <= viewport_height;
                                
                                // Get height tracker for this item
                                let tracker = self.height_trackers.entry(item_id.clone()).or_default();
                                
                                if rendered_height < viewport_height && is_fully_visible {
                                    // Small item that's fully visible - cache the height immediately
                                    let old_height = self.activity_log_height_cache.get(&item_id).copied();
                                    
                                    // Only update if change is significant (hysteresis)
                                    let should_update = if let Some(old) = old_height {
                                        (old - rendered_height).abs() > HEIGHT_CHANGE_HYSTERESIS
                                    } else {
                                        true // Always cache if we don't have a height yet
                                    };
                                    
                                    if should_update {
                                        // CRITICAL BUG: When old_height is None, we shouldn't use 0.0 as the old height
                                        // because the scroll position was calculated using the ESTIMATED height, not 0!
                                        // For now, only adjust scroll if we had a cached height before
                                        let height_diff = if let Some(old) = old_height {
                                            rendered_height - old
                                        } else {
                                            // First time caching - don't adjust scroll
                                            0.0
                                        };
                                        
                                        // If this item extends above the viewport, adjust scroll to maintain position
                                        if item_top < 0.0 && height_diff.abs() > HEIGHT_CHANGE_HYSTERESIS {
                                            self.activity_log_scroll_offset += height_diff;
                                            log::info!(
                                                "[VSCROLL] Adjusting scroll by {:.0}px for item {} above viewport (old={:.0}, new={:.0}, was_cached={})",
                                                height_diff, item_id, old_height.unwrap_or(0.0), rendered_height, old_height.is_some()
                                            );
                                        }
                                        
                                        self.activity_log_height_cache.insert(item_id.clone(), rendered_height);
                                        tracker.seen_full_height = true;
                                        tracker.measured_height = Some(rendered_height);
                                        
                                        if let Some(old) = old_height {
                                            log::info!(
                                                "[VSCROLL] Height updated for {}: {:.0}px -> {:.0}px (delta: {:.1}px)",
                                                item_id, old, rendered_height, rendered_height - old
                                            );
                                        } else {
                                            log::debug!("[VSCROLL] Height cached for {}: {:.0}px", item_id, rendered_height);
                                        }
                                    }
                                } else if rendered_height >= viewport_height {
                                    // Tall item - mark for scroll-based measurement
                                    let scroll_tracked_height = self.height_trackers.get(&item_id)
                                        .and_then(|t| t.measured_height);
                                    
                                    log::info!(
                                        "[VSCROLL] Tall item {} comparison - Rendered: {:.0}px, Scroll-tracked: {:?}, Viewport: {:.0}px",
                                        item_id, rendered_height, scroll_tracked_height, viewport_height
                                    );
                                } else {
                                    // Item is partially visible - don't cache potentially clipped height
                                    log::trace!(
                                        "Item {} partially visible (top: {}, bottom: {}), skipping cache",
                                        item_id, item_top, item_bottom
                                    );
                                }
                            }
                        }
                    }

                            log::debug!(
                                "Updated {} height cache entries from rendered elements (total cache size: {})",
                                item_elements.len(),
                                self.activity_log_height_cache.len()
                            );
                            
                            // Drop filtered_items by ending this scope
                        }
                    }
                }
            }
        }
        
        
        // STICKY BOTTOM DISABLED - This feature was causing scroll jumps because:
        // 1. It uses hardcoded line_height (20.0) vs actual font metrics
        // 2. It recalculates total height with different parameters than render_activity_log
        // 3. This causes massive jumps (3000+ pixels) when heights don't match
        // 4. It doesn't have access to proper font metrics to calculate correctly
        /*
        // Handle sticky bottom
        if sticky_bottom_needed {
                    let viewport_height = self.activity_log_scrollbar
                        .as_ref()
                        .map(|s| s.viewport_height)
                        .unwrap_or(400.0);
                    
                    // Recalculate filtered items for total height
                    let filtered_items: Vec<(usize, &ActivityItem)> = self
                        .activity_log
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| match self.activity_filter {
                            ActivityFilter::All => true,
                            ActivityFilter::Commands => matches!(item, ActivityItem::Command { .. }),
                            ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                            ActivityFilter::Suggestions => matches!(item, ActivityItem::Suggestion { .. }),
                        })
                        .collect();
                    
                    // CRITICAL: These values MUST match what's used in render_activity_log!
                    let line_height = 20.0; // This might not match the actual line height!
                    let available_width = self.activity_log_last_width.unwrap_or(300.0);
                    
                    log::debug!(
                        "[VSCROLL] Sticky bottom params: line_height={:.0}, width={:.0}, items={}",
                        line_height, available_width, filtered_items.len()
                    );
                    
                    let new_total_height = self.calculate_total_activity_log_height(
                        &filtered_items,
                        line_height,
                        available_width
                    );
                    
                    let new_max_scroll = (new_total_height - viewport_height).max(0.0);
            if (self.activity_log_scroll_offset - new_max_scroll).abs() > 1.0 {
                log::warn!(
                    "[VSCROLL] STICKY BOTTOM JUMP: scroll {} -> {} (total_height={:.0}, viewport={:.0})",
                    self.activity_log_scroll_offset, new_max_scroll, new_total_height, viewport_height
                );
                self.activity_log_scroll_offset = new_max_scroll;
            }
        }
        */
    }
}

// Static helper functions for virtual scrolling

/// Render an activity item (static version for use in closures)
fn render_activity_item_static(
    item: &ActivityItem,
    fonts: &SidebarFonts,
    idx: usize,
    palette: &wezterm_term::color::ColorPalette,
) -> Element {
    // This is a static version of render_activity_item that doesn't need &mut self
    match item {
        ActivityItem::Command {
            command,
            status,
            output,
            expanded,
            ..
        } => {
            let status_icon = match status {
                CommandStatus::Running => "▶",
                CommandStatus::Success => "✓",
                CommandStatus::Failed(_) => "✗",
            };

            let status_color = match status {
                CommandStatus::Running => LinearRgba::with_components(1.0, 1.0, 0.0, 1.0),
                CommandStatus::Success => LinearRgba::with_components(0.0, 1.0, 0.0, 1.0),
                CommandStatus::Failed(_) => LinearRgba::with_components(1.0, 0.0, 0.0, 1.0),
            };

            let mut children = vec![Element::new(
                &fonts.body,
                ElementContent::Text(format!("{} $ {}", status_icon, command)),
            )
            .colors(ElementColors {
                text: status_color.into(),
                ..Default::default()
            })
            .display(DisplayType::Block)];

            if *expanded {
                if let Some(output) = output {
                    children.push(
                        Element::new(&fonts.body, ElementContent::Text(output.clone()))
                            .colors(ElementColors {
                                text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                                ..Default::default()
                            })
                            .padding(BoxDimension {
                                left: Dimension::Pixels(16.0),
                                ..Default::default()
                            })
                            .display(DisplayType::Block),
                    );
                }
            }

            Element::new(&fonts.body, ElementContent::Children(children))
                .display(DisplayType::Block)
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        }
        ActivityItem::Chat {
            message, is_user, ..
        } => {
            if *is_user {
                Element::new(&fonts.body, ElementContent::WrappedText(message.clone()))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        bg: LinearRgba::with_components(0.2, 0.2, 0.3, 0.3).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(8.0)))
                    .display(DisplayType::Block)
                    .margin(BoxDimension {
                        left: Dimension::Pixels(40.0),
                        right: Dimension::Pixels(8.0),
                        top: Dimension::Pixels(4.0),
                        bottom: Dimension::Pixels(4.0),
                    })
            } else {
                // AI messages - render with markdown
                MarkdownRenderer::render_with_fonts(
                    message, fonts, None, // max_width
                )
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
                .display(DisplayType::Block)
                .margin(BoxDimension {
                    left: Dimension::Pixels(8.0),
                    right: Dimension::Pixels(40.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                })
            }
        }
        ActivityItem::Suggestion { title, content, .. } => {
            MarkdownRenderer::render_with_fonts(
                &format!("**{}**\n\n{}", title, content),
                fonts,
                None, // max_width
            )
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        }
        ActivityItem::Goal { text, .. } => {
            Element::new(&fonts.body, ElementContent::Text(format!("Goal: {}", text)))
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.8, 0.8, 0.8, 1.0).into(),
                    ..Default::default()
                })
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        }
    }
}

/// Estimate the height of an activity item
fn estimate_activity_item_height(
    item: &ActivityItem,
    line_height: f32,
    available_width: f32,
) -> f32 {
    // Base padding (top + bottom)
    let padding = 16.0;

    match item {
        ActivityItem::Command {
            output, expanded, ..
        } => {
            // Command line height + padding
            let mut height = line_height + padding;

            // Add output height if expanded
            if *expanded {
                if let Some(output) = output {
                    let lines = output.lines().count() as f32;
                    height += lines * line_height + 16.0; // Extra padding for output
                }
            }

            height
        }
        ActivityItem::Chat {
            message, is_user, ..
        } => {
            // Estimate wrapped text height
            let margin = if *is_user { 48.0 } else { 48.0 }; // Left or right margin
            let effective_width = available_width - margin - padding;
            let avg_char_width = line_height * 0.6; // Approximate

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                message,
                effective_width,
                avg_char_width,
            );

            lines * line_height + padding + 8.0 // Extra margin
        }
        ActivityItem::Suggestion { content, .. } => {
            // Suggestions can be quite long with markdown
            let effective_width = available_width - padding;
            let avg_char_width = line_height * 0.6;

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                content,
                effective_width,
                avg_char_width,
            );

            // Add extra for markdown formatting overhead
            lines * line_height * 1.2 + padding
        }
        ActivityItem::Goal { text, .. } => {
            // Simple text with "Goal: " prefix
            let effective_width = available_width - padding;
            let avg_char_width = line_height * 0.6;
            let full_text = format!("Goal: {}", text);

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                &full_text,
                effective_width,
                avg_char_width,
            );

            lines * line_height + padding
        }
    }
}
