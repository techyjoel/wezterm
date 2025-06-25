use super::components::{
    Card, CardState, Chip, ChipSize, ChipStyle, MarkdownRenderer, MultilineTextInput,
    ScrollableContainer, ScrollbarInfo,
};
use super::{Sidebar, SidebarConfig, SidebarPosition};
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
};
use crate::termwindow::render::scrollbar_renderer::{ScrollbarOrientation, ScrollbarRenderer};
use crate::termwindow::UIItemType;
use ::window::color::LinearRgba;
use anyhow::Result;
use config::{Dimension, DimensionContext};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use termwiz::input::KeyCode;
use wezterm_font::LoadedFont;
use window::{MouseEvent, MouseEventKind as WMEK, MousePress, PixelUnit};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    Idle,
    Thinking,
    GatheringData,
    NeedsApproval,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

pub struct CurrentSuggestion {
    title: String,
    content: String,
    has_action: bool,
    action_type: Option<String>, // "run", "dismiss", etc
}

pub struct AiSidebar {
    config: SidebarConfig,
    visible: bool,
    width: u16,

    // UI State
    agent_mode: AgentMode,
    agent_mode_enabled: bool,
    high_risk_mode_enabled: bool,
    activity_filter: ActivityFilter,

    // Data
    pub current_goal: Option<CurrentGoal>,
    pub current_suggestion: Option<CurrentSuggestion>,
    activity_log: Vec<ActivityItem>,

    // UI Components
    chat_input: MultilineTextInput,

    // Scrollbar info for external rendering
    activity_log_scrollbar: Option<ScrollbarInfo>,

    // Scrollbar renderer for handling events
    activity_log_scrollbar_renderer: Option<ScrollbarRenderer>,

    // Scrollbar bounds for hit testing
    activity_log_scrollbar_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Scroll state
    activity_log_scroll_offset: f32,

    // UI element bounds for hit testing
    filter_chip_bounds: Vec<(ActivityFilter, euclid::Rect<f32, window::PixelUnit>)>,

    // Sidebar position for coordinate conversion
    sidebar_x_position: f32,
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
            activity_log_scrollbar: None,
            activity_log_scrollbar_renderer: None,
            activity_log_scrollbar_bounds: None,
            activity_log_scroll_offset: 0.0,
            filter_chip_bounds: Vec::new(),
            sidebar_x_position: 0.0,
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

        // Set a current suggestion
        self.current_suggestion = Some(CurrentSuggestion {
            title: "Install missing dependency".to_string(),
            content: "It looks like the linker couldn't find OpenSSL. Install it with: `brew install openssl@3` and rerun `make`.".to_string(),
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
            expanded: false,
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat1".to_string(),
            message: "Sounds good, please run that.".to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(30),
        });

        // Add AI response with markdown
        self.activity_log.push(ActivityItem::Chat {
            id: "chat2".to_string(),
            message: r#"I see you're getting an **OpenSSL error**. This is a common issue when building projects. Here's how to fix it:

## Solution

1. First, check if OpenSSL is installed:
   ```bash
   brew list openssl
   ```

2. If not installed, run:
   ```bash
   brew install openssl
   ```

3. Then set the environment variables:
   ```bash
   export OPENSSL_DIR=$(brew --prefix openssl)
   export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"
   ```

4. Try running `make` again.

### Alternative Solution
If the above doesn't work, you might need to install `pkg-config`:
```bash
brew install pkg-config
```"#.to_string(),
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
    }

    fn render_header(&self, font: &Rc<LoadedFont>) -> Element {
        let title = Element::new(font, ElementContent::Text("CLiBuddy AI".to_string()))
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

        Element::new(font, ElementContent::Children(vec![title]))
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

    fn render_filter_chips(&mut self, font: &Rc<LoadedFont>) -> Element {
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
                    .render(font)
            })
            .collect();

        Element::new(font, ElementContent::Children(chips))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(8.0),
            })
    }

    fn render_status_chip(&self, font: &Rc<LoadedFont>) -> Element {
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
            .render(font);

        Element::new(font, ElementContent::Children(vec![chip]))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(4.0),
            })
    }

    fn render_current_goal(&self, font: &Rc<LoadedFont>) -> Option<Element> {
        let goal = self.current_goal.as_ref()?;

        let mut content = vec![];

        // Goal text
        let goal_text = if goal.is_editing {
            // Show edit input
            Element::new(font, ElementContent::Text(format!("{}_", &goal.edit_text)))
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                    bg: LinearRgba::with_components(0.15, 0.15, 0.17, 1.0).into(),
                    ..Default::default()
                })
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        } else {
            Element::new(font, ElementContent::Text(goal.text.clone()))
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
                .render(font);
            actions.push(confirm_btn);
        }

        if !goal.is_editing {
            let edit_btn = Chip::new("✎".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(font);
            actions.push(edit_btn);
        } else {
            let save_btn = Chip::new("Save".to_string())
                .with_style(ChipStyle::Primary)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(font);
            let cancel_btn = Chip::new("Cancel".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(font);
            actions.push(save_btn);
            actions.push(cancel_btn);
        }

        let card = Card::new()
            .with_title("Current Goal".to_string())
            .with_content(content)
            .with_actions(actions)
            .render(font);

        Some(
            Element::new(font, ElementContent::Children(vec![card]))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(16.0),
                    right: Dimension::Pixels(16.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                }),
        )
    }

    fn render_current_suggestion(&self, font: &Rc<LoadedFont>) -> Option<Element> {
        let suggestion = self.current_suggestion.as_ref()?;

        let content = vec![MarkdownRenderer::render(&suggestion.content, font)
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))];

        let mut actions = vec![];

        if suggestion.has_action {
            let run_btn = Chip::new("▶ Run".to_string())
                .with_style(ChipStyle::Success)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .render(font);
            let dismiss_btn = Chip::new("✕ Dismiss".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .render(font);
            actions.push(run_btn);
            actions.push(dismiss_btn);
        }

        let card = Card::new()
            .with_title(suggestion.title.clone())
            .with_content(content)
            .with_actions(actions)
            .render(font);

        Some(
            Element::new(font, ElementContent::Children(vec![card]))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(16.0),
                    right: Dimension::Pixels(16.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                }),
        )
    }

    pub fn render_activity_item(&self, item: &ActivityItem, font: &Rc<LoadedFont>) -> Element {
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
                    font,
                    ElementContent::Text(format!("{} {}", status_icon, command)),
                )
                .colors(ElementColors {
                    text: status_color.into(),
                    ..Default::default()
                })];

                if *expanded && output.is_some() {
                    content.push(
                        Element::new(font, ElementContent::Text(output.as_ref().unwrap().clone()))
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

                Card::new().with_content(content).render(font)
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
                    Element::new(font, ElementContent::Text(message.clone())).colors(
                        ElementColors {
                            text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                            ..Default::default()
                        },
                    )
                } else {
                    // AI messages use markdown rendering
                    MarkdownRenderer::render(message, font)
                };

                Element::new(font, ElementContent::Children(vec![content]))
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
            ActivityItem::Suggestion { title, content, .. } => Card::new()
                .with_title(format!("Past: {}", title))
                .with_content(vec![MarkdownRenderer::render(content, font)])
                .render(font),
            ActivityItem::Goal { text, .. } => {
                Element::new(font, ElementContent::Text(format!("Goal: {}", text)))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.8, 0.8, 0.8, 1.0).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(8.0)))
            }
        }
    }

    /// Get filtered activity items based on current filter
    fn render_activity_log(&mut self, font: &Rc<LoadedFont>, available_height: f32) -> Element {
        let filtered_items: Vec<&ActivityItem> = self
            .activity_log
            .iter()
            .filter(|item| match self.activity_filter {
                ActivityFilter::All => true,
                ActivityFilter::Commands => matches!(item, ActivityItem::Command { .. }),
                ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                ActivityFilter::Suggestions => matches!(item, ActivityItem::Suggestion { .. }),
            })
            .collect();

        let mut rendered_items: Vec<Element> = Vec::new();

        // Render the actual items
        // Note: Items already have proper display types and margins, no need to wrap
        rendered_items.extend(
            filtered_items
                .into_iter()
                .map(|item| self.render_activity_item(item, font)),
        );

        let rendered_items_count = rendered_items.len();
        log::debug!(
            "Rendering activity log: {} items filtered, {} items rendered",
            self.activity_log.len(),
            rendered_items_count
        );

        // Create scrollable container with pixel-based viewport height
        let viewport_height = available_height;

        log::debug!(
            "Activity log: available_height={}, viewport_height={}, total_items={}",
            available_height,
            viewport_height,
            rendered_items_count
        );

        // Use pixel-based height for scrollable container
        // Get actual font metrics for accurate height calculations
        let font_metrics = font.metrics();
        let line_height = font_metrics.cell_height.get() as f32;
        let font_context = DimensionContext {
            dpi: 96.0,
            pixel_cell: line_height,
            pixel_max: viewport_height,
        };

        let mut scrollable_container = ScrollableContainer::new_with_pixel_height(viewport_height)
            .with_font_context(font_context)
            .with_content(rendered_items)
            .with_auto_hide_scrollbar(false); // Always show scrollbar for debugging

        // CRITICAL: Set scroll position AFTER content is set, so the container can validate the offset
        scrollable_container.set_scroll_offset(self.activity_log_scroll_offset);

        log::debug!(
            "Setting scroll offset on container: offset={}, items={}",
            self.activity_log_scroll_offset,
            rendered_items_count
        );

        // Store scrollbar info for external rendering
        let scrollbar_info = scrollable_container.get_scrollbar_info();
        self.activity_log_scrollbar = Some(scrollbar_info.clone());

        // Create/update scrollbar renderer if needed
        if scrollbar_info.should_show {
            // Use the new pixel-based values from ScrollbarInfo
            let total_size = scrollbar_info.content_height;
            let viewport_size = scrollbar_info.viewport_height;
            let scroll_offset = scrollbar_info.scroll_offset;

            match &mut self.activity_log_scrollbar_renderer {
                Some(renderer) => {
                    renderer.update(total_size, viewport_size, scroll_offset);
                }
                None => {
                    self.activity_log_scrollbar_renderer = Some(ScrollbarRenderer::new_vertical(
                        total_size,
                        viewport_size,
                        scroll_offset,
                        20.0, // min thumb size
                    ));
                }
            }
        } else {
            self.activity_log_scrollbar_renderer = None;
        }

        let scrollable = scrollable_container.render(font);

        // Return the scrollable without extra padding since it's handled by the container
        scrollable
    }

    fn render_chat_input(&self, font: &Rc<LoadedFont>) -> Element {
        let input_field = self.chat_input.render(font);

        let send_button = Chip::new("Send".to_string())
            .with_style(ChipStyle::Primary)
            .with_size(ChipSize::Medium)
            .clickable(true)
            .render(font);

        Element::new(
            font,
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
        font: &Rc<LoadedFont>,
        window_height: f32,
    ) -> Element {
        // Get the dynamic bounds for the activity log
        let bounds = self
            .get_activity_log_bounds(window_height)
            .unwrap_or_else(|| {
                euclid::rect(16.0, 200.0, self.width as f32 - 32.0, window_height - 320.0)
            });

        // The activity log height is the bounds height
        let available_for_log = bounds.size.height;

        // Render the activity log content
        let activity_log = self.render_activity_log(font, available_for_log);

        // Wrap in a container with background color
        let container = Element::new(font, ElementContent::Children(vec![activity_log]))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.03, 0.03, 0.035, 1.0).into(), // Slightly lighter than sidebar
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(bounds.size.width)))
            .min_height(Some(Dimension::Pixels(bounds.size.height)));

        container
    }

    pub fn render_content(&mut self, font: &Rc<LoadedFont>, window_height: f32) -> Element {
        let mut children = vec![];

        // Fixed height elements at top
        // Header
        children.push(self.render_header(font));

        // Status chip
        children.push(self.render_status_chip(font));

        // Filter chips
        children.push(self.render_filter_chips(font));

        // Current goal card
        if let Some(goal_element) = self.render_current_goal(font) {
            children.push(goal_element);
        }

        // Current suggestion card
        if let Some(suggestion_element) = self.render_current_suggestion(font) {
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
            Element::new(font, ElementContent::Text(String::new()))
                .display(DisplayType::Block)
                .min_height(Some(Dimension::Pixels(spacer_height)))
                // Completely transparent - no background
                .colors(ElementColors {
                    bg: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0).into(),
                    ..Default::default()
                }),
        );

        // Fixed height chat input at bottom
        children.push(self.render_chat_input(font));

        // Container - transparent so the hole works
        Element::new(font, ElementContent::Children(children))
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(self.width as f32)))
            .min_height(Some(Dimension::Pixels(window_height)))
    }

    pub fn handle_filter_click(&mut self, filter: ActivityFilter) {
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

    /// Update filter chip bounds with sidebar position offset
    pub fn update_filter_chip_bounds(&mut self, sidebar_x: f32) {
        // Store sidebar position for mouse event handling
        self.sidebar_x_position = sidebar_x;

        // Clear and recalculate bounds with sidebar position
        self.filter_chip_bounds.clear();

        // These positions are relative to the sidebar's origin
        let filters = vec![
            ("All", ActivityFilter::All),
            ("Commands", ActivityFilter::Commands),
            ("Chat", ActivityFilter::Chat),
            ("Suggestions", ActivityFilter::Suggestions),
        ];

        let base_x = 16.0; // left padding within sidebar
        let base_y = 106.0; // Approximate Y position
        let chip_height = 24.0;
        let chip_spacing = 8.0;
        let chip_widths = vec![35.0, 75.0, 40.0, 85.0];

        let mut current_x = base_x + sidebar_x; // Add sidebar offset
        for ((_, filter), width) in filters.iter().zip(chip_widths.iter()) {
            let bounds = euclid::rect(current_x, base_y, *width, chip_height);
            self.filter_chip_bounds.push((*filter, bounds));
            current_x += width + chip_spacing;
        }

        log::debug!("Updated filter chip bounds with sidebar_x={}", sidebar_x);
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

    /// Check which filter chip was clicked based on coordinates
    fn get_clicked_filter(&self, event: &MouseEvent, sidebar_x: f32) -> Option<ActivityFilter> {
        // Check if click is in the filter chip area (approximate Y range)
        let y = event.coords.y as f32;
        if y < 90.0 || y > 130.0 {
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
        if x < 35.0 {
            Some(ActivityFilter::All)
        } else if x < 118.0 {
            // 35 + 8 + 75
            Some(ActivityFilter::Commands)
        } else if x < 166.0 {
            // 118 + 8 + 40
            Some(ActivityFilter::Chat)
        } else if x < 259.0 {
            // 166 + 8 + 85
            Some(ActivityFilter::Suggestions)
        } else {
            None
        }
    }
}

impl Sidebar for AiSidebar {
    fn render(&mut self, font: &Rc<LoadedFont>, window_height: f32) -> Element {
        self.render_content(font, window_height)
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
                let scroll_speed = 40.0; // Pixels per scroll step
                let scroll_amount = scroll_speed * (*amount as f32).abs();

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

                log::debug!(
                    "Scroll wheel: old_offset={}, new_offset={}, max_scroll={}, amount={}, scroll_amount={}",
                    old_offset, self.activity_log_scroll_offset, max_scroll, amount, scroll_amount
                );
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

        // Check for filter chip clicks
        match event.kind {
            WMEK::Press(MousePress::Left) => {
                if let Some(filter) = self.get_clicked_filter(event, self.sidebar_x_position) {
                    log::debug!("Filter chip clicked: {:?}", filter);
                    self.activity_filter = filter;
                    return Ok(true);
                }

                // Log unhandled clicks for debugging
                log::debug!(
                    "Unhandled sidebar click at: ({}, {})",
                    event.coords.x,
                    event.coords.y
                );

                // Return false to indicate we didn't handle it
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn handle_key_event(&mut self, key: &KeyCode) -> Result<bool> {
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
