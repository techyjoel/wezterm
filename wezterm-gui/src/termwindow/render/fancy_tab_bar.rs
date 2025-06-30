use crate::customglyph::*;
use crate::tabbar::{TabBarItem, TabEntry};
use crate::termwindow::box_model::*;
use crate::termwindow::render::window_buttons::window_button_element;
use crate::termwindow::{UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::{Dimension, DimensionContext, TabBarColors};
use std::rc::Rc;
use wezterm_font::LoadedFont;
use wezterm_term::color::{ColorAttribute, ColorPalette};
use window::color::LinearRgba;
use window::{IntegratedTitleButtonAlignment, IntegratedTitleButtonStyle};

const X_BUTTON: &[Poly] = &[
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::One, BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::Zero, BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::One, BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
];

const PLUS_BUTTON: &[Poly] = &[
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Frac(1, 2), BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::Frac(1, 2), BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Frac(1, 2)),
            PolyCommand::LineTo(BlockCoord::One, BlockCoord::Frac(1, 2)),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
];

impl crate::TermWindow {
    pub fn invalidate_fancy_tab_bar(&mut self) {
        self.fancy_tab_bar.take();
    }

    pub fn build_fancy_tab_bar(&self, palette: &ColorPalette) -> anyhow::Result<ComputedElement> {
        let tab_bar_height = self.tab_bar_pixel_height()?;
        let font = self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let items = self.tab_bar.items();
        let colors = self
            .config
            .colors
            .as_ref()
            .and_then(|c| c.tab_bar.as_ref())
            .cloned()
            .unwrap_or_else(TabBarColors::default);

        let mut left_status = vec![];
        let mut left_eles = vec![];
        let mut right_eles = vec![];
        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let bar_colors = ElementColors {
            border: BorderColor::default(),
            bg: if self.focused.is_some() {
                self.config.window_frame.active_titlebar_bg
            } else {
                self.config.window_frame.inactive_titlebar_bg
            }
            .to_linear()
            .mul_alpha(if window_is_transparent {
                self.config.window_background_opacity
            } else {
                1.0
            })
            .into(),
            text: if self.focused.is_some() {
                self.config.window_frame.active_titlebar_fg
            } else {
                self.config.window_frame.inactive_titlebar_fg
            }
            .to_linear()
            .into(),
        };

        let item_to_elem = |item: &TabEntry, tab_bar_height: f32| -> Element {
            let is_active = matches!(item.item, TabBarItem::Tab { active: true, .. });
            let bg_color = item
                .title
                .get_cell(0)
                .and_then(|c| match c.attrs().background() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_bg(col)),
                });
            let fg_color = item
                .title
                .get_cell(0)
                .and_then(|c| match c.attrs().foreground() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_fg(col)),
                });

            let new_tab = colors.new_tab();
            let new_tab_hover = colors.new_tab_hover();
            let active_tab = colors.active_tab();
            let inactive_tab_hover = colors.inactive_tab_hover();

            let fg_active = fg_color
                .unwrap_or_else(|| active_tab.fg_color.into())
                .to_linear();
            let fg_inactive = fg_color
                .unwrap_or_else(|| colors.inactive_tab().fg_color.into())
                .to_linear();
            let fg_inactive_hover = fg_color
                .unwrap_or_else(|| inactive_tab_hover.fg_color.into())
                .to_linear();

            // Create the text element with transparent background
            let text_element = Element::with_line_transparent_bg(&font, &item.title, palette)
                .vertical_align(VerticalAlign::Middle)
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::TRANSPARENT.into(),
                    text: if is_active { fg_active } else { fg_inactive }.into(),
                })
                // Add hover colors for inactive tabs
                .hover_colors({
                    if is_active {
                        None
                    } else {
                        Some(ElementColors {
                            border: BorderColor::default(),
                            bg: LinearRgba::TRANSPARENT.into(),
                            text: fg_inactive_hover.into(),
                        })
                    }
                });

            // Wrap text in a container so vertical_align works
            let element = Element::new(&font, ElementContent::Children(vec![text_element]));

            match item.item {
                TabBarItem::RightStatus | TabBarItem::LeftStatus | TabBarItem::None => element
                    .item_type(UIItemType::TabBar(TabBarItem::None))
                    .line_height(Some(1.75))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.),
                        bottom: Dimension::Cells(0.),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(0.)))
                    .colors(bar_colors.clone()),
                TabBarItem::NewTabButton => {
                    // Create the poly element
                    let poly_elem = Element::new(
                        &font,
                        ElementContent::Poly {
                            line_width: metrics.underline_height.max(2),
                            poly: SizedPoly {
                                poly: PLUS_BUTTON,
                                width: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                                height: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                            },
                        },
                    )
                    .transparent_bg()
                    .vertical_align(VerticalAlign::Middle);

                    // Wrap in a container with transparent background
                    Element::new(&font, ElementContent::Children(vec![poly_elem]))
                        .item_type(UIItemType::TabBar(item.item.clone()))
                        .vertical_align(VerticalAlign::Middle)
                        .margin(BoxDimension {
                            left: Dimension::Cells(0.0),
                            right: Dimension::Cells(0.),
                            top: Dimension::Cells(0.0),
                            bottom: Dimension::Cells(0.),
                        })
                        .padding(BoxDimension {
                            left: Dimension::Cells(0.5),
                            right: Dimension::Cells(0.5),
                            top: Dimension::Cells(0.0),
                            bottom: Dimension::Cells(0.0),
                        })
                        .border(BoxDimension::new(Dimension::Pixels(1.)))
                        .colors(ElementColors {
                            border: BorderColor::new(new_tab.bg_color.to_linear()),
                            bg: new_tab.bg_color.to_linear().into(),
                            text: new_tab.fg_color.to_linear().into(),
                        })
                        .hover_colors(Some(ElementColors {
                            border: BorderColor::new(new_tab.bg_color.to_linear()),
                            bg: new_tab_hover.bg_color.to_linear().into(),
                            text: new_tab_hover.fg_color.to_linear().into(),
                        }))
                }
                .min_height(Some(Dimension::Pixels(tab_bar_height - 2.))), // Adjust to match tabs
                TabBarItem::Tab { active, .. } if active => element
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.5),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.0),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .colors(ElementColors {
                        border: BorderColor {
                            left: new_tab.bg_color.to_linear(),
                            right: new_tab.bg_color.to_linear(),
                            top: new_tab.bg_color.to_linear(),
                            bottom: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0), // transparent bottom
                        },
                        bg: bg_color
                            .unwrap_or_else(|| active_tab.bg_color.into())
                            .to_linear()
                            .into(),
                        text: fg_active.into(),
                    })
                    .min_height(Some(Dimension::Pixels(tab_bar_height - 1.))), // Adjust to border height
                TabBarItem::Tab { .. } => element
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.5),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.0),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .colors({
                        let inactive_tab = colors.inactive_tab();
                        let bg = bg_color
                            .unwrap_or_else(|| inactive_tab.bg_color.into())
                            .to_linear();
                        ElementColors {
                            border: BorderColor {
                                left: new_tab.bg_color.to_linear(),
                                right: new_tab.bg_color.to_linear(),
                                top: new_tab.bg_color.to_linear(),
                                bottom: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0), // transparent bottom
                            },
                            bg: bg.into(),
                            text: fg_inactive.into(),
                        }
                    })
                    .hover_colors({
                        Some(ElementColors {
                            border: BorderColor {
                                left: new_tab.bg_color.to_linear(),
                                right: new_tab.bg_color.to_linear(),
                                top: new_tab.bg_color.to_linear(),
                                bottom: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0), // transparent bottom
                            },
                            bg: bg_color
                                .unwrap_or_else(|| inactive_tab_hover.bg_color.into())
                                .to_linear()
                                .into(),
                            text: fg_inactive_hover.into(),
                        })
                    })
                    .min_height(Some(Dimension::Pixels(tab_bar_height - 1.))), // Adjust to border height
                TabBarItem::WindowButton(button) => window_button_element(
                    button,
                    self.window_state.contains(window::WindowState::MAXIMIZED),
                    &font,
                    &metrics,
                    &self.config,
                ),
            }
        };

        let num_tabs: f32 = items
            .iter()
            .map(|item| match item.item {
                TabBarItem::NewTabButton | TabBarItem::Tab { .. } => 1.,
                _ => 0.,
            })
            .sum();
        let max_tab_width = ((self.dimensions.pixel_width as f32 / num_tabs)
            - (1.5 * metrics.cell_size.width as f32))
            .max(0.);

        // Reserve space for the native titlebar buttons
        if self
            .config
            .window_decorations
            .contains(::window::WindowDecorations::INTEGRATED_BUTTONS)
            && self.config.integrated_title_button_style == IntegratedTitleButtonStyle::MacOsNative
            && !self.window_state.contains(window::WindowState::FULL_SCREEN)
        {
            left_status.push(
                Element::new(&font, ElementContent::Text("".to_string())).margin(BoxDimension {
                    left: Dimension::Cells(4.0), // FIXME: determine exact width of macos ... buttons
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                }),
            );
        }

        for item in items {
            match item.item {
                TabBarItem::LeftStatus => left_status.push(item_to_elem(item, tab_bar_height)),
                TabBarItem::None | TabBarItem::RightStatus => {
                    right_eles.push(item_to_elem(item, tab_bar_height))
                }
                TabBarItem::WindowButton(_) => {
                    if self.config.integrated_title_button_alignment
                        == IntegratedTitleButtonAlignment::Left
                    {
                        left_eles.push(item_to_elem(item, tab_bar_height))
                    } else {
                        right_eles.push(item_to_elem(item, tab_bar_height))
                    }
                }
                TabBarItem::Tab { tab_idx, active } => {
                    let mut elem = item_to_elem(item, tab_bar_height);
                    elem.max_width = Some(Dimension::Pixels(max_tab_width));
                    elem.content = match elem.content {
                        ElementContent::Text(_) => unreachable!(),
                        ElementContent::WrappedText(_) => unreachable!(),
                        ElementContent::Poly { .. } => unreachable!(),
                        ElementContent::Children(mut kids) => {
                            if self.config.show_close_tab_button_in_tabs {
                                kids.push(make_x_button(&font, &metrics, &colors, tab_idx, active));
                            }
                            ElementContent::Children(kids)
                        }
                    };
                    left_eles.push(elem);
                }
                _ => left_eles.push(item_to_elem(item, tab_bar_height)),
            }
        }

        // Calculate tab bar width to account for sidebar
        let padding = self.effective_right_padding(&self.config) as f32;
        let sidebar_manager = self.sidebar_manager.borrow();
        let sidebar_expansion = sidebar_manager.get_window_expansion() as f32;
        drop(sidebar_manager);

        // Tab bar should end where the terminal content ends
        let tab_bar_width = if sidebar_expansion > 0.0 {
            self.dimensions.pixel_width as f32 - sidebar_expansion - padding
        } else {
            self.dimensions.pixel_width as f32 - padding
        };

        let mut children = vec![];

        if !left_status.is_empty() {
            children.push(
                Element::new(&font, ElementContent::Children(left_status))
                    .colors(bar_colors.clone()),
            );
        }

        let window_buttons_at_left = self
            .config
            .window_decorations
            .contains(window::WindowDecorations::INTEGRATED_BUTTONS)
            && (self.config.integrated_title_button_alignment
                == IntegratedTitleButtonAlignment::Left
                || self.config.integrated_title_button_style
                    == IntegratedTitleButtonStyle::MacOsNative);

        let left_padding = if window_buttons_at_left {
            if self.config.integrated_title_button_style == IntegratedTitleButtonStyle::MacOsNative
            {
                if !self.window_state.contains(window::WindowState::FULL_SCREEN) {
                    Dimension::Pixels(70.0)
                } else {
                    Dimension::Cells(0.5)
                }
            } else {
                Dimension::Pixels(0.0)
            }
        } else {
            Dimension::Cells(0.0)
        };

        // Create a horizontal layout with tabs on left and fill on right
        let mut tab_row = vec![];

        // Add tabs (transparent background)
        tab_row.push(
            Element::new(&font, ElementContent::Children(left_eles))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::TRANSPARENT.into(), // transparent background for tabs area
                    text: bar_colors.text.clone(),
                })
                .padding(BoxDimension {
                    left: left_padding,
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                }),
        );

        // Add background fill element to the right of tabs
        tab_row.push(
            Element::new(&font, ElementContent::Text("".to_string()))
                .colors(bar_colors.clone())
                .min_width(Some(Dimension::Pixels(tab_bar_width))) // Fill remaining space
                .min_height(Some(Dimension::Pixels(tab_bar_height))), // add 1px to match borders on elements
        );

        // Add the tab row container
        children.push(
            Element::new(&font, ElementContent::Children(tab_row))
                .vertical_align(VerticalAlign::Bottom),
        );

        // Add right elements (sidebar button) as floating
        children.push(
            Element::new(&font, ElementContent::Children(right_eles))
                .colors(bar_colors.clone())
                .float(Float::Right),
        );

        let content = ElementContent::Children(children);

        let tabs = Element::new(&font, content)
            .display(DisplayType::Block)
            .item_type(UIItemType::TabBar(TabBarItem::None))
            .min_width(Some(Dimension::Pixels(tab_bar_width)))
            .min_height(Some(Dimension::Pixels(tab_bar_height)))
            .vertical_align(VerticalAlign::Bottom)
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: LinearRgba::TRANSPARENT.into(), // Transparent container
                text: bar_colors.text,
            });

        let border = self.get_os_border();

        let mut computed = self.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_height as f32,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_width as f32,
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(
                    border.left.get() as f32,
                    0.,
                    tab_bar_width - border.left.get() as f32,
                    tab_bar_height,
                ),
                metrics: &metrics,
                gl_state: self.render_state.as_ref().unwrap(),
                zindex: 10,
            },
            &tabs,
        )?;

        computed.translate(euclid::vec2(
            0.,
            if self.config.tab_bar_at_bottom {
                self.dimensions.pixel_height as f32
                    - (computed.bounds.height() + border.bottom.get() as f32)
            } else {
                border.top.get() as f32
            },
        ));

        Ok(computed)
    }

    pub fn paint_fancy_tab_bar(&self) -> anyhow::Result<Vec<UIItem>> {
        let computed = self.fancy_tab_bar.as_ref().ok_or_else(|| {
            anyhow::anyhow!("paint_fancy_tab_bar called but fancy_tab_bar is None")
        })?;
        let ui_items = computed.ui_items();

        let gl_state = self.render_state.as_ref().unwrap();
        self.render_element(&computed, gl_state, None)?;

        Ok(ui_items)
    }
}

fn make_x_button(
    font: &Rc<LoadedFont>,
    metrics: &RenderMetrics,
    colors: &TabBarColors,
    tab_idx: usize,
    active: bool,
) -> Element {
    Element::new(
        &font,
        ElementContent::Poly {
            line_width: metrics.underline_height.max(2),
            poly: SizedPoly {
                poly: X_BUTTON,
                width: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                height: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
            },
        },
    )
    // Ensure that we draw our background over the
    // top of the rest of the tab contents
    .zindex(1)
    .vertical_align(VerticalAlign::Middle)
    .float(Float::Right)
    .item_type(UIItemType::CloseTab(tab_idx))
    .colors(ElementColors {
        border: BorderColor::default(),
        bg: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0).into(), // transparent by default
        text: (if active {
            colors.active_tab().fg_color
        } else {
            colors.inactive_tab().fg_color
        })
        .to_linear()
        .into(),
    })
    .hover_colors({
        let inactive_tab_hover = colors.inactive_tab_hover();
        let active_tab = colors.active_tab();

        Some(ElementColors {
            border: BorderColor::default(),
            bg: (if active {
                inactive_tab_hover.bg_color
            } else {
                colors.inactive_tab_hover().bg_color
            })
            .to_linear()
            .into(), // show background on hover
            text: (if active {
                inactive_tab_hover.fg_color
            } else {
                active_tab.fg_color
            })
            .to_linear()
            .into(),
        })
    })
    .padding(BoxDimension {
        left: Dimension::Cells(0.25),
        right: Dimension::Cells(0.25),
        top: Dimension::Cells(0.25),
        bottom: Dimension::Cells(0.25),
    })
    .margin(BoxDimension {
        left: Dimension::Cells(0.5),
        right: Dimension::Cells(0.),
        top: Dimension::Cells(0.),
        bottom: Dimension::Cells(0.),
    })
}
