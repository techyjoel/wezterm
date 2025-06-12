use super::components::{Card, CardState, Chip, ChipSize, ChipStyle, ScrollableContainer};
use super::{Sidebar, SidebarConfig, SidebarPosition};
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
};
use crate::termwindow::UIItemType;
use ::window::color::LinearRgba;
use anyhow::Result;
use config::Dimension;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use termwiz::input::{KeyCode, MouseEvent};
use wezterm_font::LoadedFont;

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
    current_goal: Option<CurrentGoal>,
    current_suggestion: Option<CurrentSuggestion>,
    activity_log: Vec<ActivityItem>,

    // UI Components
    chat_input: String,
    chat_input_cursor: usize,
    scroll_position: usize,
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
            chat_input: String::new(),
            chat_input_cursor: 0,
            scroll_position: 0,
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

    fn render_activity_item(&self, item: &ActivityItem, font: &Rc<LoadedFont>) -> Element {
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
                                left: Dimension::Pixels(24.0),
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

                Element::new(font, ElementContent::Text(message.clone()))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(12.0)))
                    .margin(BoxDimension {
                        left: if *is_user {
                            Dimension::Pixels(40.0)
                        } else {
                            Dimension::Pixels(0.0)
                        },
                        right: if *is_user {
                            Dimension::Pixels(0.0)
                        } else {
                            Dimension::Pixels(40.0)
                        },
                        bottom: Dimension::Pixels(8.0),
                        ..Default::default()
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.0)))
                    .colors(ElementColors {
                        border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 0.5)),
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        bg: bg_color.into(),
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

    fn render_activity_log(&self, font: &Rc<LoadedFont>) -> Element {
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
            .map(|item| self.render_activity_item(item, font))
            .collect();

        let scrollable = ScrollableContainer::new(20) // Approx viewport height
            .with_content(rendered_items)
            .render(font);

        Element::new(font, ElementContent::Children(vec![scrollable]))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(8.0),
            })
            .min_height(Some(Dimension::Pixels(200.0)))
    }

    fn render_chat_input(&self, font: &Rc<LoadedFont>) -> Element {
        let input_text = if self.chat_input.is_empty() {
            "Type a message...".to_string()
        } else {
            format!("{}_", &self.chat_input)
        };

        let text_color = if self.chat_input.is_empty() {
            LinearRgba::with_components(0.5, 0.5, 0.5, 1.0)
        } else {
            LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
        };

        let input_field = Element::new(font, ElementContent::Text(input_text))
            .colors(ElementColors {
                text: text_color.into(),
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(12.0)))
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .colors(ElementColors {
                border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 0.5)),
                text: text_color.into(),
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
            })
            .margin(BoxDimension {
                right: Dimension::Pixels(8.0),
                ..Default::default()
            })
            .min_height(Some(Dimension::Pixels(40.0)));

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

    pub fn render_content(&self, font: &Rc<LoadedFont>) -> Element {
        let mut children = vec![];

        // Header
        children.push(self.render_header(font));

        // Filter chips
        children.push(self.render_filter_chips(font));

        // Status chip
        children.push(self.render_status_chip(font));

        // Current goal card
        if let Some(goal_element) = self.render_current_goal(font) {
            children.push(goal_element);
        }

        // Current suggestion card
        if let Some(suggestion_element) = self.render_current_suggestion(font) {
            children.push(suggestion_element);
        }

        // Activity log
        children.push(self.render_activity_log(font));

        // Chat input
        children.push(self.render_chat_input(font));

        // Container
        Element::new(font, ElementContent::Children(children))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.05, 0.05, 0.06, 1.0).into(),
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(self.width as f32)))
            .min_height(Some(Dimension::Pixels(800.0))) // Full height
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
        self.chat_input.insert(self.chat_input_cursor, c);
        self.chat_input_cursor += 1;
    }

    pub fn handle_chat_send(&mut self) {
        if !self.chat_input.is_empty() {
            let message = self.chat_input.clone();
            self.activity_log.push(ActivityItem::Chat {
                id: format!("chat_{}", self.activity_log.len()),
                message,
                is_user: true,
                timestamp: SystemTime::now(),
            });
            self.chat_input.clear();
            self.chat_input_cursor = 0;
        }
    }
}

impl Sidebar for AiSidebar {
    fn render(&mut self) {
        // Rendering is handled by render_content() method
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

    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> Result<bool> {
        // TODO: Implement mouse event handling
        Ok(false)
    }

    fn handle_key_event(&mut self, key: &KeyCode) -> Result<bool> {
        match key {
            KeyCode::Char(c) => {
                self.handle_chat_input(*c);
                Ok(true)
            }
            KeyCode::Enter => {
                self.handle_chat_send();
                Ok(true)
            }
            KeyCode::Backspace => {
                if self.chat_input_cursor > 0 {
                    self.chat_input.remove(self.chat_input_cursor - 1);
                    self.chat_input_cursor -= 1;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
