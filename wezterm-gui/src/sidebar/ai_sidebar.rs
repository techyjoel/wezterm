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
use config::Dimension;
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

#[derive(Debug, Clone, PartialEq)]
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

    fn render_filter_chips(&self, font: &Rc<LoadedFont>) -> Element {
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

        let content = vec![
            Element::new(font, ElementContent::Text(suggestion.content.clone()))
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                    ..Default::default()
                })
                .padding(BoxDimension::new(Dimension::Pixels(8.0))),
        ];

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
                .with_content(vec![Element::new(
                    font,
                    ElementContent::Text(content.clone()),
                )
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                    ..Default::default()
                })])
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

        let rendered_items: Vec<Element> = filtered_items
            .into_iter()
            .map(|item| {
                // Wrap each item in a block container to ensure vertical stacking
                Element::new(
                    font,
                    ElementContent::Children(vec![self.render_activity_item(item, font)]),
                )
                .display(DisplayType::Block)
            })
            .collect();

        // Create scrollable container with pixel-based viewport height
        let viewport_height = available_height - 32.0; // Account for padding (16px top + 16px bottom)

        log::debug!(
            "Activity log: available_height={}, viewport_height={}, total_items={}",
            available_height,
            viewport_height,
            rendered_items.len()
        );

        // Use pixel-based height for scrollable container
        let mut scrollable_container = ScrollableContainer::new_with_pixel_height(viewport_height);

        // Set scroll position
        scrollable_container.set_scroll_offset(self.activity_log_scroll_offset);

        scrollable_container = scrollable_container
            .with_content(rendered_items)
            .with_auto_hide_scrollbar(false); // Always show scrollbar for debugging

        // Store scrollbar info for external rendering
        let scrollbar_info = scrollable_container.get_scrollbar_info();
        self.activity_log_scrollbar = Some(scrollbar_info.clone());

        // Create/update scrollbar renderer if needed
        if scrollbar_info.should_show {
            let total_size = scrollbar_info.total_items as f32 * 40.0; // Approximate item height
            let viewport_size = scrollbar_info.viewport_items as f32 * 40.0;
            let scroll_offset = scrollbar_info.scroll_offset as f32 * 40.0;

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

        // Create the activity log with padding
        let padded_scrollable = Element::new(font, ElementContent::Children(vec![scrollable]))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(8.0),
            })
            .min_height(Some(Dimension::Pixels(available_height)));

        padded_scrollable
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

        // Calculate fixed heights for elements ABOVE the activity log:
        // Header: 50px (text + padding)
        // Status chip: 40px (chip + padding)
        // Filter chips: 50px (chips + padding)
        // Extra for testing: 350px
        let top_fixed_height = 490.0;

        // Add optional card heights
        let goal_height = if self.current_goal.is_some() {
            140.0
        } else {
            0.0
        };
        let suggestion_height = if self.current_suggestion.is_some() {
            160.0
        } else {
            0.0
        };

        // Fixed height for elements BELOW the activity log:
        // Chat input: 120px (3 lines + button + padding)
        let bottom_fixed_height = 120.0;

        // Calculate available height for activity log
        let total_fixed = top_fixed_height + goal_height + suggestion_height + bottom_fixed_height;
        let available_for_log = (window_height - total_fixed).max(50.0);

        log::debug!(
            "Sidebar height calculation: window_height={}, top_fixed={}, goal={}, suggestion={}, bottom_fixed={}, total_fixed={}, available_for_log={}",
            window_height, top_fixed_height, goal_height, suggestion_height, bottom_fixed_height, total_fixed, available_for_log
        );

        // Activity log with dynamic height
        children.push(self.render_activity_log(font, available_for_log));

        // Fixed height chat input at bottom
        children.push(self.render_chat_input(font));

        // Container
        Element::new(font, ElementContent::Children(children))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.05, 0.05, 0.06, 1.0).into(),
                ..Default::default()
            })
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
        self.activity_log_scrollbar_bounds = Some(bounds);
    }

    /// Check if a mouse event is within the scrollbar bounds
    fn is_scrollbar_event(&self, event: &MouseEvent) -> bool {
        if let Some(bounds) = &self.activity_log_scrollbar_bounds {
            let point = euclid::point2(event.coords.x as f32, event.coords.y as f32);
            bounds.contains(point)
        } else {
            false
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
        // Handle scroll wheel events
        if let WMEK::VertWheel(amount) = &event.kind {
            // Check if we have a scrollbar renderer to get scroll metrics
            if let Some(_renderer) = &self.activity_log_scrollbar_renderer {
                let scroll_speed = 40.0; // Pixels per scroll step
                let scroll_amount = scroll_speed * (*amount as f32);

                let new_offset = if *amount > 0 {
                    // Scroll up
                    (self.activity_log_scroll_offset - scroll_amount).max(0.0)
                } else {
                    // Scroll down
                    self.activity_log_scroll_offset + scroll_amount
                };

                // Constrain to valid range
                // TODO: Get actual content height from scrollbar info
                self.activity_log_scroll_offset = new_offset;
                log::debug!(
                    "Scroll wheel updated offset to: {}",
                    self.activity_log_scroll_offset
                );
                return Ok(true);
            }
        }

        // Check if this is a scrollbar event
        if self.is_scrollbar_event(event) {
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

        // Handle other sidebar clicks
        match event.kind {
            WMEK::Press(MousePress::Left) => {
                // Log the click position for debugging
                log::info!(
                    "Sidebar clicked at: ({}, {})",
                    event.coords.x,
                    event.coords.y
                );

                // TODO: Map coordinates to specific elements like filter chips, buttons, etc.
                // For now, just toggle the filter as a test
                self.activity_filter = match self.activity_filter {
                    ActivityFilter::All => ActivityFilter::Commands,
                    ActivityFilter::Commands => ActivityFilter::Chat,
                    ActivityFilter::Chat => ActivityFilter::Suggestions,
                    ActivityFilter::Suggestions => ActivityFilter::All,
                };

                Ok(true) // Indicate we handled the event
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
