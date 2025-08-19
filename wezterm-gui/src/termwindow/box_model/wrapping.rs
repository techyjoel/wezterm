//! Text wrapping functionality for the box model system

use super::types::*;
use crate::customglyph::BlockKey;
use crate::termwindow::{RenderState, TermWindowNotif};
use ::window::WindowOps;
use anyhow::Result;
use config::TextStyle;
use finl_unicode::grapheme_clusters::Graphemes;
use std::rc::Rc;
use termwiz::cell::grapheme_column_width;
use wezterm_font::shaper::{Direction, GlyphInfo};
use wezterm_font::LoadedFont;

impl crate::TermWindow {
    pub(crate) fn wrap_text(
        &self,
        text: &str,
        font: &Rc<LoadedFont>,
        max_width: f32,
        context: &LayoutContext,
        style: &TextStyle,
    ) -> Result<Vec<Vec<ElementCell>>> {
        let (lines, _) = self.wrap_text_with_info(text, font, max_width, context, style, None)?;
        Ok(lines)
    }

    /// Wraps text and returns both cells and line information
    pub(crate) fn wrap_text_with_info(
        &self,
        text: &str,
        font: &Rc<LoadedFont>,
        max_width: f32,
        context: &LayoutContext,
        style: &TextStyle,
        global_byte_offset: Option<usize>,
    ) -> Result<(Vec<Vec<ElementCell>>, Vec<WrappedLine>)> {
        // Step 1: Calculate line breaks and create WrappedLines
        let wrapped_lines =
            self.wrap_text_into_lines(text, font, max_width, context, style, global_byte_offset)?;

        // Step 2: Shape each complete line
        let mut all_lines = Vec::new();
        let track_cluster = context.source == RenderSource::Sidebar;

        log::debug!(
            "wrap_text_with_info: RenderSource={:?}, track_cluster={}",
            context.source,
            track_cluster
        );

        for wrapped_line in &wrapped_lines {
            if wrapped_line.shaped_text.is_empty() {
                // Preserve empty lines
                all_lines.push(Vec::new());
                continue;
            }

            // Shape the complete line text
            log::debug!(
                "Shaping line text: '{}', len={}",
                wrapped_line.shaped_text,
                wrapped_line.shaped_text.len()
            );
            let window = self.window.as_ref().unwrap().clone();
            let infos = font.shape(
                &wrapped_line.shaped_text,
                move || window.notify(TermWindowNotif::InvalidateShapeCache),
                BlockKey::filter_out_synthetic,
                None,
                wezterm_bidi::Direction::LeftToRight,
                None,
                None,
            )?;

            log::debug!(
                "Shape result: {} glyphs for {} chars",
                infos.len(),
                wrapped_line.shaped_text.chars().count()
            );

            // Log the last few cluster values
            if infos.len() >= 3 {
                for i in (infos.len() - 3)..infos.len() {
                    log::debug!(
                        "  Glyph {}: cluster={}, glyph_pos={}",
                        i,
                        infos[i].cluster,
                        infos[i].glyph_pos
                    );
                }
            }

            // Debug logging for cluster values (only in trace mode)
            if track_cluster && log::log_enabled!(log::Level::Trace) {
                log::trace!(
                    "Shaping line: '{}' (byte_offset={}, shaped_offset={})",
                    wrapped_line.shaped_text,
                    wrapped_line.byte_offset,
                    wrapped_line.shaped_offset
                );
                for (i, info) in infos.iter().take(5).enumerate() {
                    log::trace!(
                        "  Glyph[{}]: cluster={}, num_cells={}, x_advance={}",
                        i,
                        info.cluster,
                        info.num_cells,
                        info.x_advance.get()
                    );
                }
            }

            let cells = self.shape_text_to_cells(
                &wrapped_line.shaped_text,
                &infos,
                font,
                context,
                style,
                track_cluster,
            )?;

            all_lines.push(cells);
        }

        Ok((all_lines, wrapped_lines))
    }

    /// Wraps text into lines, creating proper WrappedLine structures
    pub fn wrap_text_into_lines(
        &self,
        text: &str,
        font: &Rc<LoadedFont>,
        max_width: f32,
        context: &LayoutContext,
        style: &TextStyle,
        global_byte_offset: Option<usize>,
    ) -> Result<Vec<WrappedLine>> {
        let mut wrapped_lines = Vec::new();
        // Always start at 0 for element-local offsets
        // The global_byte_offset is set on the Element, not on individual lines
        let mut byte_offset = 0;

        // Calculate the average character width for this font once, to use for wrapping estimates
        let avg_char_width = self.calculate_average_char_width(font, context, style)?;

        log::debug!(
            "wrap_text_into_lines: text='{}', text.len()={}, max_width={}, avg_char_width={}",
            text.replace('\n', "\\n"),
            text.len(),
            max_width,
            avg_char_width
        );

        // Split by newlines first to preserve line structure
        for line_text in text.lines() {
            let line_byte_start = byte_offset;
            let line_byte_len = line_text.len();

            if line_text.is_empty() {
                // Preserve empty lines
                wrapped_lines.push(WrappedLine {
                    byte_offset: line_byte_start,
                    byte_end: line_byte_start,
                    skip_leading_spaces: false,
                    leading_space_bytes: 0,
                    shaped_text: String::new(),
                    shaped_offset: 0,
                });
                byte_offset += 1; // Account for newline
                continue;
            }

            // Track position within this line
            let mut current_pos = 0; // byte position within line
            let mut current_width = 0.0;
            let mut line_count = 0; // track if this is a continuation line
            let mut current_line_start = 0; // where current wrapped line starts
            let mut current_line_text = String::new();

            // Calculate initial indentation
            let leading_spaces = line_text.len() - line_text.trim_start().len();
            if leading_spaces > 0 {
                // Calculate indentation width
                let indent_str = &line_text[..leading_spaces];
                let window = self.window.as_ref().unwrap().clone();
                let infos = font.shape(
                    indent_str,
                    move || window.notify(TermWindowNotif::InvalidateShapeCache),
                    BlockKey::filter_out_synthetic,
                    None,
                    wezterm_bidi::Direction::LeftToRight,
                    None,
                    None,
                )?;
                current_width =
                    self.calculate_text_width(indent_str, &infos, font, context, style)?;
                current_line_text.push_str(indent_str);
                current_pos = leading_spaces;
            }

            // Process the rest of the line
            while current_pos < line_byte_len {
                // For continuation lines, skip leading spaces
                if line_count > 0 {
                    let remaining = &line_text[current_pos..];
                    let trimmed = remaining.trim_start();
                    let skip_spaces = remaining.len() - trimmed.len();

                    if skip_spaces > 0 {
                        // Create wrapped line for previous content
                        if !current_line_text.is_empty() {
                            wrapped_lines.push(WrappedLine {
                                byte_offset: line_byte_start + current_line_start,
                                byte_end: line_byte_start + current_pos,
                                skip_leading_spaces: false,
                                leading_space_bytes: 0,
                                shaped_text: current_line_text.clone(),
                                shaped_offset: 0,
                            });
                        }

                        // Start new line after skipped spaces
                        current_pos += skip_spaces;
                        current_line_start = current_pos;
                        current_line_text.clear();
                        current_width = 0.0;
                    }
                }

                // Find next wrap point
                let mut last_space_pos = None;
                let mut last_space_width = current_width;
                let mut wrap_pos = current_pos;
                let mut line_width = current_width;

                // Look for wrap point character by character
                for (idx, ch) in line_text[current_pos..].char_indices() {
                    let char_pos = current_pos + idx;

                    // Estimate character width using the calculated average for this font
                    // This gives much more accurate wrapping for proportional fonts
                    // These multipliers are based on typical proportional font metrics:
                    // - Spaces are ~30-40% of average width in most fonts (we use 0.6 to be conservative)
                    // - Uppercase letters and punctuation are ~10-20% wider than average
                    // - Lowercase letters and digits cluster around the average
                    let char_width = if ch == ' ' {
                        avg_char_width * 0.6  // Spaces are typically narrower
                    } else if ch.is_uppercase() || ch.is_ascii_punctuation() {
                        avg_char_width * 1.1  // Uppercase and punctuation are typically wider
                    } else {
                        avg_char_width        // Use the calculated average for most characters
                    };

                    // Check if adding this character would exceed width
                    if line_width + char_width > max_width && line_width > 0.0 {
                        // Need to wrap
                        if let Some(space_pos) = last_space_pos {
                            // Wrap at last space
                            wrap_pos = space_pos;
                        } else {
                            // No space found, wrap at current position
                            wrap_pos = char_pos;
                        }
                        break;
                    }

                    line_width += char_width;
                    wrap_pos = char_pos + ch.len_utf8();

                    if ch == ' ' {
                        last_space_pos = Some(char_pos + ch.len_utf8());
                        last_space_width = line_width;
                    }
                }

                // Add text up to wrap point to current line
                current_line_text.push_str(&line_text[current_pos..wrap_pos]);

                // Create wrapped line
                if !current_line_text.is_empty() || line_count == 0 {
                    wrapped_lines.push(WrappedLine {
                        byte_offset: line_byte_start + current_line_start,
                        byte_end: line_byte_start + wrap_pos,
                        skip_leading_spaces: line_count > 0,
                        leading_space_bytes: if line_count > 0 {
                            current_pos - current_line_start
                        } else {
                            0
                        },
                        shaped_text: current_line_text.clone(),
                        shaped_offset: 0,
                    });
                }

                // Move to next line
                line_count += 1;

                // Skip the space that caused the wrap if present
                if wrap_pos < line_byte_len && line_text.as_bytes()[wrap_pos] == b' ' {
                    current_pos = wrap_pos + 1;
                } else {
                    current_pos = wrap_pos;
                }

                current_line_start = current_pos;
                current_line_text.clear();
                current_width = 0.0;
            }

            // Handle any remaining text
            if current_pos > current_line_start && current_line_start < line_byte_len {
                let remaining_text = &line_text[current_line_start..];
                if !remaining_text.is_empty() {
                    wrapped_lines.push(WrappedLine {
                        byte_offset: line_byte_start + current_line_start,
                        byte_end: line_byte_start + line_byte_len,
                        skip_leading_spaces: line_count > 0,
                        leading_space_bytes: 0,
                        shaped_text: remaining_text.to_string(),
                        shaped_offset: 0,
                    });
                }
            }

            byte_offset = line_byte_start + line_byte_len + 1; // +1 for newline
        }

        // Handle case where text doesn't end with newline
        if byte_offset == text.len() + 1 && !text.is_empty() && !text.ends_with('\n') {
            // We over-counted by 1
            // This is OK - the byte offsets are still correct
        }

        Ok(wrapped_lines)
    }

    /// Wraps styled text with per-span colors and optional fonts
    ///
    /// # Parameters
    /// - `element_colors`: The element's colors to use as default for unstyled text segments.
    ///   This parameter was added to fix a bug where unstyled text would inherit transparent
    ///   color when there was no parent element to inherit from.
    pub fn wrap_styled_text(
        &self,
        text: &str,
        default_font: &Rc<LoadedFont>,
        style_spans: &[StyleSpan],
        max_width: f32,
        context: &LayoutContext,
        default_style: &TextStyle,
        element_colors: &ElementColors,
        global_byte_offset: Option<usize>,
    ) -> Result<(
        Vec<Vec<ElementCell>>,
        Vec<Vec<ElementColors>>,
        Vec<Vec<Option<FontStyleFlags>>>,
        Vec<WrappedLine>,
    )> {
        // Check if this is monospace-only content (e.g., code blocks)
        // Code blocks have no font variants and no font overrides
        let is_monospace_only = style_spans.iter().all(|span| span.is_monospace());

        // Step 1: Calculate character width based on content type
        let char_width = if is_monospace_only {
            // For monospace content (code blocks), use the font's cell width directly
            // This avoids expensive text shaping for width calculation
            default_font.metrics().cell_width.get() as f32
        } else {
            // For variable-width text, calculate average width from a sample
            self.calculate_average_char_width(default_font, context, default_style)?
        };

        let wrapped_lines =
            self.wrap_text_with_estimates(text, char_width, max_width, global_byte_offset);

        // Clone wrapped_lines to return them along with the shaped content
        let wrapped_lines_for_return = wrapped_lines.clone();

        // Step 2: Shape each wrapped line with appropriate fonts
        let mut shaped_lines = Vec::new();
        for line in &wrapped_lines {
            let shaped = self.shape_line_with_styles(
                line,
                style_spans,
                default_font,
                context,
                default_style,
                text,
            )?;
            shaped_lines.push(shaped);
        }

        // Step 3: Build style mappings
        // Create the style mapper for colors
        let mut mapper = AsciiStyleMapper::new(text);
        // Track wrapping using the shaped lines with line info to handle skipped spaces
        mapper.track_wrapping_with_lines(&shaped_lines, &wrapped_lines);

        // Build per-cell styles and font styles
        let mut line_styles = Vec::new();
        let mut line_font_styles = Vec::new();
        // Use the element's colors as default for unstyled text
        let default_colors = element_colors.clone();

        for (line_idx, line) in shaped_lines.iter().enumerate() {
            let mut line_colors = Vec::new();
            let mut line_fonts = Vec::new();

            for (cell_idx, _cell) in line.iter().enumerate() {
                let (colors, font_style) =
                    mapper.get_style_for_cell(line_idx, cell_idx, style_spans, &default_colors);
                line_colors.push(colors);
                line_fonts.push(font_style);
            }

            line_styles.push(line_colors);
            line_font_styles.push(line_fonts);
        }

        Ok((
            shaped_lines,
            line_styles,
            line_font_styles,
            wrapped_lines_for_return,
        ))
    }

    /// Calculate average character width for a font by measuring representative characters
    pub(crate) fn calculate_average_char_width(
        &self,
        font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &TextStyle,
    ) -> Result<f32> {
        let font_id = font.id();

        // Check cache first
        if let Some(width) = FONT_WIDTH_CACHE.with(|cache| cache.borrow().get(&font_id).copied()) {
            return Ok(width);
        }

        // Use a representative sample of characters to estimate average width
        // This includes common letters, digits, punctuation, and spaces based on English text frequency
        // Using a realistic sample that reflects typical English text distribution
        // This gives more accurate width estimates than a simple character set
        const SAMPLE_TEXT: &str = "the quick brown fox jumps over the lazy dog. this is a sample of typical english text with common words and spacing patterns that better represents actual usage in sidebars.";

        // Shape the sample text
        let window = self.window.as_ref().unwrap().clone();
        let infos = font.shape(
            SAMPLE_TEXT,
            move || window.notify(TermWindowNotif::InvalidateShapeCache),
            BlockKey::filter_out_synthetic,
            None,
            Direction::LeftToRight,
            None,
            None,
        )?;

        // Calculate total width
        let total_width = self.calculate_text_width(SAMPLE_TEXT, &infos, font, context, style)?;

        // Return average width per character
        let raw_avg_width = total_width / SAMPLE_TEXT.len() as f32;

        // Apply a correction factor based on empirical testing
        // This increase helps prevent wrapping issues in non-monospace text
        // where our sample text underestimates the average width of actual content
        let width_correction_factor = get_width_correction_factor();
        let avg_width = raw_avg_width * width_correction_factor;

        // Trace logging to understand width calculation
        log::trace!(
            "CHAR_WIDTH_CALC: font={:?}, total_width={:.2}, sample_len={}, raw_avg={:.2}, correction={:.2}, final_avg={:.2}",
            font.id(),
            total_width,
            SAMPLE_TEXT.len(),
            raw_avg_width,
            width_correction_factor,
            avg_width
        );

        // Store in cache with simple eviction policy
        FONT_WIDTH_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();

            // Simple eviction: clear cache if it gets too large
            if cache.len() >= FONT_WIDTH_CACHE_SIZE {
                cache.clear();
            }

            cache.insert(font_id, avg_width);
        });

        Ok(avg_width)
    }

    /// Helper to calculate text width from shaped glyphs
    pub(crate) fn calculate_text_width(
        &self,
        text: &str,
        infos: &[GlyphInfo],
        font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &TextStyle,
    ) -> Result<f32> {
        let mut width = 0.0;
        let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();

        for info in infos {
            // Check if it's a unicode block glyph
            let cell_start = &text[info.cluster as usize..];
            let mut iter = Graphemes::new(cell_start).peekable();
            if let Some(grapheme) = iter.next() {
                if let Some(_key) = BlockKey::from_str(grapheme) {
                    width += context.width.pixel_cell;
                    continue;
                }

                let followed_by_space = iter.peek() == Some(&" ");
                let num_cells = grapheme_column_width(grapheme, None);
                let glyph = glyph_cache.cached_glyph(
                    info,
                    style,
                    followed_by_space,
                    font,
                    context.metrics,
                    num_cells as u8,
                )?;
                width += glyph.x_advance.get() as f32;
            }
        }

        Ok(width)
    }

    /// Helper to get width of a single ElementCell
    pub(crate) fn get_cell_width(&self, cell: &ElementCell, context: &LayoutContext) -> Result<f32> {
        match cell {
            ElementCell::Sprite(_) => Ok(context.width.pixel_cell),
            ElementCell::Glyph(glyph) => Ok(glyph.x_advance.get() as f32),
            ElementCell::GlyphWithCluster { glyph, .. } => Ok(glyph.x_advance.get() as f32),
        }
    }

    /// Helper to convert shaped text to ElementCells with cluster offset adjustment
    pub(crate) fn shape_text_to_cells_with_offset(
        &self,
        text: &str,
        infos: &[GlyphInfo],
        font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &TextStyle,
        track_cluster: bool,
        cluster_offset: u32,
    ) -> Result<Vec<ElementCell>> {
        let cells = self.shape_text_to_cells(text, infos, font, context, style, track_cluster)?;

        // Adjust clusters for styled text segments to maintain continuous numbering
        // Only apply this for segments within a line, not for global offsets
        if cluster_offset > 0 && track_cluster {
            // Debug: Show cluster adjustment for first few glyphs
            if !cells.is_empty() {
                // Log the first and last cluster values before adjustment
                let first_cluster = cells.iter().find_map(|c| match c {
                    ElementCell::GlyphWithCluster { cluster, .. } => Some(*cluster),
                    _ => None,
                });
                let last_cluster = cells.iter().rev().find_map(|c| match c {
                    ElementCell::GlyphWithCluster { cluster, .. } => Some(*cluster),
                    _ => None,
                });
                if let (Some(first), Some(last)) = (first_cluster, last_cluster) {
                    log::debug!(
                        "  Adjusting clusters: text_len={}, offset={}, clusters: {}..{} → {}..{}",
                        text.len(),
                        cluster_offset,
                        first,
                        last,
                        first + cluster_offset,
                        last + cluster_offset
                    );
                }
            }

            let adjusted_cells: Vec<ElementCell> = cells
                .into_iter()
                .enumerate()
                .map(|(idx, cell)| match cell {
                    ElementCell::GlyphWithCluster { glyph, cluster } => {
                        let adjusted_cluster = cluster + cluster_offset;
                        if idx < 3 {
                            log::debug!(
                                "  📍 Adjusting cluster: {} + {} = {}",
                                cluster,
                                cluster_offset,
                                adjusted_cluster
                            );
                        }
                        ElementCell::GlyphWithCluster {
                            glyph,
                            cluster: adjusted_cluster,
                        }
                    }
                    other => other,
                })
                .collect();
            Ok(adjusted_cells)
        } else {
            Ok(cells)
        }
    }

    /// Helper to convert shaped text to ElementCells
    pub(crate) fn shape_text_to_cells(
        &self,
        text: &str,
        infos: &[GlyphInfo],
        font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &TextStyle,
        track_cluster: bool,
    ) -> Result<Vec<ElementCell>> {
        log::trace!(
            "shape_text_to_cells: text='{}', text.len()={}, infos.len()={}, track_cluster={}",
            text,
            text.len(),
            infos.len(),
            track_cluster
        );
        let mut cells = Vec::new();
        let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();

        for (idx, info) in infos.iter().enumerate() {
            // Check if it's a unicode block glyph
            if info.cluster as usize >= text.len() {
                log::warn!(
                    "shape_text_to_cells: Glyph {} has cluster {} but text.len()={}. Skipping.",
                    idx,
                    info.cluster,
                    text.len()
                );
                continue;
            }
            let cell_start = &text[info.cluster as usize..];
            let mut iter = Graphemes::new(cell_start).peekable();
            if let Some(grapheme) = iter.next() {
                if let Some(key) = BlockKey::from_str(grapheme) {
                    let sprite = glyph_cache.cached_block(key, context.metrics)?;
                    cells.push(ElementCell::Sprite(sprite));
                    continue;
                }

                let followed_by_space = iter.peek() == Some(&" ");
                let num_cells = grapheme_column_width(grapheme, None);
                let glyph = glyph_cache.cached_glyph(
                    info,
                    style,
                    followed_by_space,
                    font,
                    context.metrics,
                    num_cells as u8,
                )?;

                if track_cluster {
                    // For sidebar text, preserve the cluster information
                    cells.push(ElementCell::GlyphWithCluster {
                        glyph,
                        cluster: info.cluster,
                    });
                } else {
                    // For terminal text, use regular glyph without cluster
                    cells.push(ElementCell::Glyph(glyph));
                }
            }
        }

        Ok(cells)
    }

    /// Wraps text using uniform width estimates to determine line breaks
    pub(crate) fn wrap_text_with_estimates(
        &self,
        text: &str,
        char_width: f32,
        max_width: f32,
        global_byte_offset: Option<usize>,
    ) -> Vec<WrappedLine> {
        let mut wrapped_lines = Vec::new();
        // Always start at 0 for element-local offsets
        // The global_byte_offset is set on the Element, not on individual lines
        let mut byte_offset = 0;

        // Split by newlines first to preserve line structure
        for line_text in text.lines() {
            let line_byte_start = byte_offset;
            let line_byte_len = line_text.len();

            if line_text.is_empty() {
                // Preserve empty lines
                wrapped_lines.push(WrappedLine {
                    byte_offset: line_byte_start,
                    byte_end: line_byte_start,
                    skip_leading_spaces: false,
                    leading_space_bytes: 0,
                    shaped_text: String::new(),
                    shaped_offset: 0,
                });
                byte_offset += 1; // Account for newline
                continue;
            }

            let mut current_pos = 0; // byte position within line
            let mut current_width = 0.0;
            let mut line_count = 0; // track if this is a continuation line

            // Calculate initial indentation width (but don't skip it in position)
            let leading_spaces = line_text.len() - line_text.trim_start().len();
            if leading_spaces > 0 {
                current_width += leading_spaces as f32 * char_width;
                // Don't update current_pos - we want to include the indentation in the first line
            }

            while current_pos < line_byte_len {
                // Store the actual start position of this line segment
                let line_start_pos = current_pos;

                // For continuation lines, calculate and skip leading spaces
                let mut skip_spaces = 0;
                if line_count > 0 && current_pos < line_byte_len {
                    // Count leading spaces from the current position
                    let remaining = &line_text[current_pos..];
                    let trimmed = remaining.trim_start();
                    skip_spaces = remaining.len() - trimmed.len();

                    // Advance current_pos past spaces for wrapping calculations
                    current_pos += skip_spaces;
                }
                let mut line_width = if line_count > 0 { 0.0 } else { current_width };
                let mut last_space_pos = None;
                let mut last_space_width = current_width;

                // Find where to wrap this line
                let mut char_indices = line_text[current_pos..].char_indices();
                let mut wrap_pos = current_pos;

                for (byte_idx, ch) in char_indices {
                    let actual_pos = current_pos + byte_idx;
                    let ch_width = char_width;

                    // Check if adding this character would exceed width
                    if line_width + ch_width > max_width && line_width > 0.0 {
                        // Need to wrap
                        if let Some(space_pos) = last_space_pos {
                            // Wrap at last space
                            wrap_pos = space_pos;
                            current_width = last_space_width;
                        } else {
                            // No space found, wrap at current position
                            wrap_pos = actual_pos;
                        }
                        break;
                    }

                    line_width += ch_width;
                    wrap_pos = actual_pos + ch.len_utf8();

                    if ch == ' ' {
                        last_space_pos = Some(actual_pos + ch.len_utf8());
                        last_space_width = line_width;
                    }
                }

                // Create wrapped line
                // Calculate what text will actually be shaped
                let shaped_start = if line_count > 0 && skip_spaces > 0 {
                    line_start_pos + skip_spaces
                } else {
                    line_start_pos
                };
                let shaped_text = line_text[shaped_start..wrap_pos].to_string();

                let wrapped_line = WrappedLine {
                    byte_offset: line_byte_start + line_start_pos,
                    byte_end: line_byte_start + wrap_pos,
                    skip_leading_spaces: line_count > 0,
                    leading_space_bytes: skip_spaces,
                    shaped_text: shaped_text.clone(),
                    shaped_offset: shaped_start - line_start_pos,
                };

                // Debug logging for wrapped line creation
                if line_count > 0 && skip_spaces > 0 {
                    log::debug!(
                        "📏 Created wrapped line: line_count={}, skip_spaces={}, shaped_text='{}', line_start_pos={}, shaped_start={}",
                        line_count, skip_spaces, &shaped_text[..shaped_text.len().min(20)], line_start_pos, shaped_start
                    );
                }

                wrapped_lines.push(wrapped_line);

                // Move to next line
                line_count += 1;

                // Set position for next line
                // Skip the space that caused the wrap if present
                if wrap_pos < line_byte_len && line_text.as_bytes()[wrap_pos] == b' ' {
                    current_pos = wrap_pos + 1;
                } else {
                    current_pos = wrap_pos;
                }
                current_width = 0.0;
            }

            byte_offset = line_byte_start + line_byte_len + 1; // +1 for newline
        }

        wrapped_lines
    }
}