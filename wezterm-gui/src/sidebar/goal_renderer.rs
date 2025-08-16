//! Goal and suggestion rendering functionality for the AI sidebar

// Character width estimation for suggestion cards
const SUGGESTION_CHAR_WIDTH_MULTIPLIER: f32 = 0.4;

use crate::color::LinearRgba;
use crate::sidebar::ai_sidebar::CurrentGoal;
use crate::sidebar::components::card::Card;
use crate::sidebar::components::chip::{Chip, ChipSize, ChipStyle};
use crate::sidebar::text_selection::SelectionTarget;
use crate::sidebar::{SidebarFonts, SelectionState};
use crate::sidebar::ai_sidebar::CurrentSuggestion;
use crate::termwindow::box_model::{
    BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
};
use config::Dimension;
use crate::termwindow::UIItemType;
use std::rc::Rc;
use wezterm_font::LoadedFont;

/// Render the current goal card
pub fn render_current_goal(
    current_goal: &Option<CurrentGoal>,
    selection_state: &SelectionState,
    fonts: &SidebarFonts,
    sidebar_width: f32,
) -> Option<Element> {
    let goal = current_goal.as_ref()?;

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
        // Check if this goal has a selection
        let selection = match &selection_state.active_selection {
            Some(SelectionTarget::Goal {
                anchor_byte,
                current_byte,
            }) => Some((
                *anchor_byte.min(current_byte),
                *anchor_byte.max(current_byte),
            )),
            _ => None,
        };

        // Always use WrappedText to avoid layout changes
        log::debug!(
            "GOAL TEXT DEBUG: Rendering goal text: '{}', len={}",
            goal.text,
            goal.text.len()
        );
        let elem = Element::new(&fonts.body, ElementContent::WrappedText(goal.text.clone()));

        // Calculate available width for goal text
        let goal_content_width = sidebar_width - 40.0; // Account for padding

        log::debug!(
            "Goal width calculation: sidebar_width={}, content_width={}, has_selection={}",
            sidebar_width,
            goal_content_width,
            selection.is_some()
        );

        // Don't pre-calculate positions - they'll be extracted after rendering
        elem.item_type(UIItemType::GoalText {
            char_positions: Vec::new(), // Will be populated after rendering
        })
        .colors(ElementColors {
            text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
            ..Default::default()
        })
        .max_width(Some(Dimension::Pixels(goal_content_width)))
        .padding(BoxDimension::new(Dimension::Pixels(8.0)))
    };

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
        .with_content(vec![goal_text])
        .with_actions(actions)
        .pass_through_events(true) // Allow child UIItemTypes to be detected
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

/// Render the current suggestion card
pub fn render_current_suggestion(
    current_suggestion: &Option<CurrentSuggestion>,
    selection_state: &SelectionState,
    fonts: &SidebarFonts,
    sidebar_width: f32,
    estimate_wrapped_lines: impl Fn(&str, f32, &SidebarFonts) -> usize,
) -> Option<Element> {
    let suggestion = current_suggestion.as_ref()?;

    // Check if content would exceed 2 lines when wrapped
    const MAX_LINES: usize = 2;

    // Get approximate width available for text in the suggestion card
    // Sidebar: 16px padding each side = 32px
    // Card: 8px margin each side = 16px
    // Content container: 8px padding each side = 16px
    // Total: 32 + 16 + 16 = 64px
    let available_width = sidebar_width - 64.0;

    // Use our wrapping estimation to determine if we need truncation
    let estimated_lines = estimate_wrapped_lines(&suggestion.content, available_width, fonts);
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
        // Check if this suggestion has a selection
        let selection = match &selection_state.active_selection {
            Some(SelectionTarget::Suggestion {
                anchor_byte,
                current_byte,
            }) => Some((
                *anchor_byte.min(current_byte),
                *anchor_byte.max(current_byte),
            )),
            _ => None,
        };

        // For short content, still use fixed height
        // Selection is now rendered as an overlay, not inline styles
        let elem = Element::new(
            &fonts.body,
            ElementContent::WrappedText(suggestion.content.clone()),
        );

        // Calculate available width for suggestion text
        let suggestion_content_width = sidebar_width - 40.0; // Account for padding

        content_elements.push(
            elem.item_type(UIItemType::SuggestionText {
                char_positions: Vec::new(), // Position data extracted after rendering
            })
            .colors(ElementColors {
                text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
                ..Default::default()
            })
            .display(DisplayType::Block)
            .max_width(Some(Dimension::Pixels(suggestion_content_width)))
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
            .with_item_type(UIItemType::SuggestionRunButton)
            .render(&fonts.body);
        let dismiss_btn = Chip::new("✕ Dismiss".to_string())
            .with_style(ChipStyle::Default)
            .with_size(ChipSize::Medium)
            .clickable(true)
            .with_item_type(UIItemType::SuggestionDismissButton)
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
            .with_item_type(UIItemType::ShowMoreButton("current".to_string()))
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