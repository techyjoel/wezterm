use crate::quad::TripleLayerQuadAllocator;
use crate::termwindow::render::RenderScreenLineParams;
use crate::utilsprites::RenderMetrics;
use config::ConfigHandle;
use mux::renderable::RenderableDimensions;
use wezterm_term::color::ColorAttribute;
use window::color::LinearRgba;

impl crate::TermWindow {
    pub fn paint_tab_bar(&mut self, layers: &mut TripleLayerQuadAllocator) -> anyhow::Result<()> {
        if self.config.use_fancy_tab_bar {
            if self.fancy_tab_bar.is_none() {
                let palette = self.palette().clone();
                let tab_bar = self.build_fancy_tab_bar(&palette)?;
                self.fancy_tab_bar.replace(tab_bar);
            }

            self.ui_items.append(&mut self.paint_fancy_tab_bar()?);

            // Fill the gap between tab bar and sidebar button when sidebar is visible
            let sidebar_manager = self.sidebar_manager.borrow();
            let sidebar_expansion = sidebar_manager.get_window_expansion() as f32;
            drop(sidebar_manager);

            if sidebar_expansion > 0.0 {
                // There's a gap to fill - render a background rectangle
                let padding = self.effective_right_padding(&self.config) as f32;
                let border = self.get_os_border();
                let tab_bar_height = self.tab_bar_pixel_height()?;

                // Calculate gap position and size
                let gap_x = self.dimensions.pixel_width as f32 - sidebar_expansion - padding;
                let gap_width = sidebar_expansion + padding;
                let gap_y = if self.config.tab_bar_at_bottom {
                    self.dimensions.pixel_height as f32
                        - tab_bar_height
                        - border.bottom.get() as f32
                } else {
                    border.top.get() as f32
                };

                // Get the bar background color with transparency
                let window_is_transparent = !self.window_background.is_empty()
                    || self.config.window_background_opacity != 1.0;
                let bar_bg_color = if self.focused.is_some() {
                    self.config.window_frame.active_titlebar_bg
                } else {
                    self.config.window_frame.inactive_titlebar_bg
                }
                .to_linear()
                .mul_alpha(if window_is_transparent {
                    self.config.window_background_opacity
                } else {
                    1.0
                });

                // Render the gap fill
                self.filled_rectangle(
                    layers,
                    0,
                    euclid::rect(gap_x, gap_y, gap_width, tab_bar_height),
                    bar_bg_color,
                )?;
            }

            return Ok(());
        }

        let border = self.get_os_border();

        let palette = self.palette().clone();
        let tab_bar_height = self.tab_bar_pixel_height()?;
        let tab_bar_y = if self.config.tab_bar_at_bottom {
            ((self.dimensions.pixel_height as f32) - (tab_bar_height + border.bottom.get() as f32))
                .max(0.)
        } else {
            border.top.get() as f32
        };

        // Register the tab bar location
        self.ui_items.append(&mut self.tab_bar.compute_ui_items(
            tab_bar_y as usize,
            self.render_metrics.cell_size.height as usize,
            self.render_metrics.cell_size.width as usize,
        ));

        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let gl_state = self.render_state.as_ref().unwrap();
        let white_space = gl_state.util_sprites.white_space.texture_coords();
        let filled_box = gl_state.util_sprites.filled_box.texture_coords();
        let default_bg = palette
            .resolve_bg(ColorAttribute::Default)
            .to_linear()
            .mul_alpha(if window_is_transparent {
                0.
            } else {
                self.config.text_background_opacity
            });

        // Calculate tab bar width to match the terminal content area
        let padding = self.effective_right_padding(&self.config) as f32;
        let sidebar_manager = self.sidebar_manager.borrow();
        let sidebar_expansion = sidebar_manager.get_window_expansion() as f32;
        drop(sidebar_manager);

        // Tab bar should end where the terminal content ends
        // When sidebar is expanded (even partially), account for that expansion
        let tab_bar_width = if sidebar_expansion > 0.0 {
            self.dimensions.pixel_width as f32
                - sidebar_expansion
                - padding
                - border.right.get() as f32
        } else {
            self.dimensions.pixel_width as f32 - padding - border.right.get() as f32
        };

        // Fill tab bar background explicitly with correct width
        let tab_bar_bg_rect = euclid::rect(0.0, tab_bar_y, tab_bar_width, tab_bar_height);
        self.filled_rectangle(
            layers,
            0,
            tab_bar_bg_rect,
            palette
                .background
                .to_linear()
                .mul_alpha(self.config.window_background_opacity),
        )?;

        self.render_screen_line(
            RenderScreenLineParams {
                top_pixel_y: tab_bar_y,
                left_pixel_x: 0.,
                pixel_width: tab_bar_width,
                stable_line_idx: None,
                line: self.tab_bar.line(),
                selection: 0..0,
                cursor: &Default::default(),
                palette: &palette,
                dims: &RenderableDimensions {
                    cols: tab_bar_width as usize / self.render_metrics.cell_size.width as usize,
                    physical_top: 0,
                    scrollback_rows: 0,
                    scrollback_top: 0,
                    viewport_rows: 1,
                    dpi: self.terminal_size.dpi,
                    pixel_height: self.render_metrics.cell_size.height as usize,
                    pixel_width: self.terminal_size.pixel_width,
                    reverse_video: false,
                },
                config: &self.config,
                cursor_border_color: LinearRgba::default(),
                foreground: palette.foreground.to_linear(),
                pane: None,
                is_active: true,
                selection_fg: LinearRgba::default(),
                selection_bg: LinearRgba::default(),
                cursor_fg: LinearRgba::default(),
                cursor_bg: LinearRgba::default(),
                cursor_is_default_color: true,
                white_space,
                filled_box,
                window_is_transparent,
                default_bg,
                style: None,
                font: None,
                use_pixel_positioning: self.config.experimental_pixel_positioning,
                render_metrics: self.render_metrics,
                shape_key: None,
                password_input: false,
            },
            layers,
        )?;

        Ok(())
    }

    pub fn tab_bar_pixel_height_impl(
        config: &ConfigHandle,
        fontconfig: &wezterm_font::FontConfiguration,
        render_metrics: &RenderMetrics,
    ) -> anyhow::Result<f32> {
        if config.use_fancy_tab_bar {
            let font = fontconfig.title_font()?;
            Ok((font.metrics().cell_height.get() as f32 * 1.75).ceil())
        } else {
            Ok(render_metrics.cell_size.height as f32)
        }
    }

    pub fn tab_bar_pixel_height(&self) -> anyhow::Result<f32> {
        Self::tab_bar_pixel_height_impl(&self.config, &self.fonts, &self.render_metrics)
    }
}
