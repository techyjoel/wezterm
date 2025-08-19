//! Element operations: shaping, layout computation, and rendering

use super::types::*;
use super::wrapping;
use crate::color::LinearRgba;
use crate::customglyph::BlockKey;
use crate::glyphcache::CachedGlyph;
use crate::quad::{QuadImpl, QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::{MouseCapture, RenderState, TermWindowNotif, UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use ::window::{RectF, WindowOps};
use anyhow::{anyhow, Result};
use config::{DimensionContext, TextStyle};
use euclid::num::Zero;
use finl_unicode::grapheme_clusters::Graphemes;
use std::rc::Rc;
use termwiz::cell::{grapheme_column_width, Presentation};
use termwiz::surface::Line;
use wezterm_font::units::PixelUnit;
use wezterm_font::LoadedFont;
use wezterm_term::color::ColorPalette;
use window::bitmaps::atlas::OutOfTextureSpace;

impl crate::TermWindow {    /// Extract line text from original text, handling both local and global byte offsets
    fn extract_line_text_with_offsets<'a>(
        &self,
        line: &WrappedLine,
        original_text: &'a str,
        context: &LayoutContext,
    ) -> anyhow::Result<(&'a str, &'a str, usize)> {
        // Debug logging for wrapped line properties
        if context.source == RenderSource::Sidebar {
            // Log the first few wrapped lines to understand the text flow
            static mut LINE_COUNT: usize = 0;
            unsafe {
                if LINE_COUNT < 5 {
                    let line_text_preview = if line.byte_offset < original_text.len()
                        && line.byte_end <= original_text.len()
                    {
                        &original_text[line.byte_offset..line.byte_end.min(line.byte_offset + 50)]
                    } else {
                        "<invalid range>"
                    };
                    log::debug!(
                        "📝 Wrapped line {}: offset={}, end={}, text='{}'",
                        LINE_COUNT,
                        line.byte_offset,
                        line.byte_end,
                        line_text_preview
                    );
                    LINE_COUNT += 1;
                }
            }
        }

        // Extract the line text from the original with bounds checking
        // When global_byte_offset is used, WrappedLines have document-relative offsets
        // We need to convert them to element-relative offsets for text extraction
        let (full_line_text, local_byte_offset) =
            if line.byte_offset <= original_text.len() && line.byte_end <= original_text.len() {
                // Local offsets - can index directly into original_text
                (
                    &original_text[line.byte_offset..line.byte_end],
                    line.byte_offset,
                )
            } else {
                // Global offsets detected - need to find this line within the element's text
                // The line spans from byte_offset to byte_end in the document
                // We need to find where this maps to in our local text

                // For elements with global offsets, the entire text is passed and we need to
                // extract the appropriate portion based on the line boundaries
                // Since we don't have the element's global offset here, we'll use a different approach:
                // If the line's byte range doesn't fit in the text, it must be using global offsets

                // Check if this could be a wrapped line from text with global offsets
                let _line_length = line.byte_end - line.byte_offset;

                // Find the line within the text by matching the shaped_text
                if !line.shaped_text.is_empty() {
                    // Use the pre-calculated shaped text which has the correct content
                    let shaped_text = &line.shaped_text;
                    if let Some(pos) = original_text.find(shaped_text) {
                        let end_pos = pos + shaped_text.len();
                        (&original_text[pos..end_pos], pos)
                    } else {
                        log::error!(
                            "Could not find shaped_text '{}' in original_text",
                            &shaped_text.chars().take(20).collect::<String>()
                        );
                        return Err(anyhow::anyhow!("Could not find shaped text in original"));
                    }
                } else {
                    log::error!(
                        "Invalid byte offsets: offset={}, end={}, text_len={} (shaped_text is empty)",
                        line.byte_offset,
                        line.byte_end,
                        original_text.len()
                    );
                    return Err(anyhow::anyhow!("Invalid byte offsets with empty shaped text"));
                }
            };

        // Apply space skipping if needed
        let line_text = if line.skip_leading_spaces && line.leading_space_bytes > 0 {
            if line.leading_space_bytes > full_line_text.len() {
                log::error!(
                    "Invalid leading_space_bytes: {} > line length {}",
                    line.leading_space_bytes,
                    full_line_text.len()
                );
                full_line_text
            } else {
                &full_line_text[line.leading_space_bytes..]
            }
        } else {
            full_line_text
        };

        Ok((full_line_text, line_text, local_byte_offset))
    }

    /// Collect style spans that apply to this wrapped line
    fn collect_style_spans_for_line<'a>(
        &self,
        line: &WrappedLine,
        line_text: &str,
        full_line_text: &str,
        local_byte_offset: usize,
        style_spans: &'a [StyleSpan],
        context: &LayoutContext,
    ) -> Vec<(usize, usize, Option<&'a StyleSpan>)> {
        // Find which style spans overlap with this line
        let mut segments = Vec::new();
        let mut last_end = 0;

        // First, collect all style spans that affect this line
        let mut line_spans = Vec::new();

        // Debug: Log style spans for first line
        if context.source == RenderSource::Sidebar && line.byte_offset == 0 {
            log::debug!("📌 Style spans for first line:");
            for (i, span) in style_spans.iter().enumerate().take(5) {
                log::debug!(
                    "  Span {}: start={}, end={}, has_style={}",
                    i,
                    span.start,
                    span.end,
                    span.font_style.is_some()
                );
            }
        }

        // Calculate the effective line end for style span comparison
        let effective_byte_offset = local_byte_offset;
        let effective_byte_end = local_byte_offset + full_line_text.len();

        for span in style_spans {
            // Check if span overlaps with this line
            // IMPORTANT: Style spans are relative to the full rendered text (not markdown),
            // and wrapped lines have byte_offset/byte_end positions within that text.
            if span.end <= effective_byte_offset || span.start >= effective_byte_end {
                continue;
            }

            // Calculate overlap within the line
            // The span positions are global within the full text, so we need to make them
            // relative to this specific wrapped line
            let start_in_original = if span.start > effective_byte_offset {
                span.start - effective_byte_offset
            } else {
                0
            };

            let end_in_original = if span.end < effective_byte_end {
                span.end - effective_byte_offset
            } else {
                effective_byte_end - effective_byte_offset
            };

            // Debug: Log the span mapping for the first few spans
            if context.source == RenderSource::Sidebar && line.byte_offset < 100 {
                log::debug!(
                    "📎 Span mapping: global span [{}, {}) → line offset {} → line-relative [{}, {})",
                    span.start,
                    span.end,
                    line.byte_offset,
                    start_in_original,
                    end_in_original
                );
            }

            // Adjust positions for the space-skipped line_text
            let start_in_line = if line.skip_leading_spaces && line.leading_space_bytes > 0 {
                if start_in_original >= line.leading_space_bytes {
                    start_in_original - line.leading_space_bytes
                } else {
                    0
                }
            } else {
                start_in_original
            };

            let end_in_line = if line.skip_leading_spaces && line.leading_space_bytes > 0 {
                if end_in_original > line.leading_space_bytes {
                    (end_in_original - line.leading_space_bytes).min(line_text.len())
                } else {
                    0
                }
            } else {
                end_in_original.min(line_text.len())
            };

            // Only add if the span covers some actual text
            if start_in_line < end_in_line && end_in_line <= line_text.len() {
                line_spans.push((start_in_line, end_in_line, span));
            }
        }

        // Sort spans by start position
        line_spans.sort_by_key(|&(start, _, _)| start);

        // Build segments ensuring complete coverage
        for &(start, end, span) in &line_spans {
            // Add unstyled segment before this span if there's a gap
            if start > last_end {
                segments.push((last_end, start, None));
            }

            // Add styled segment
            segments.push((start, end, Some(span)));
            last_end = end.max(last_end);
        }

        // Add final unstyled segment if needed
        if last_end < line_text.len() {
            segments.push((last_end, line_text.len(), None));
        }

        // If no segments, shape entire line with default font
        if segments.is_empty() {
            segments.push((0, line_text.len(), None));
        }

        segments
    }

    /// Shape a single text segment into cells
    fn shape_segment_to_cells(
        &self,
        segment_text: &str,
        start: usize,
        style_span: Option<&StyleSpan>,
        line: &WrappedLine,
        default_font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &config::TextStyle,
    ) -> anyhow::Result<Vec<ElementCell>> {
        let font = if let Some(span) = style_span {
            if let Some(span_font) = &span.font {
                span_font
            } else {
                default_font
            }
        } else {
            default_font
        };

        // Shape the segment
        let window = self.window.as_ref().unwrap().clone();
        let infos = font.shape(
            segment_text,
            move || window.notify(TermWindowNotif::InvalidateShapeCache),
            BlockKey::filter_out_synthetic,
            None,
            wezterm_bidi::Direction::LeftToRight,
            None,
            None,
        )?;

        // Convert to cells with cluster offset adjustment for continuous numbering
        let track_cluster = context.source == RenderSource::Sidebar;

        // CRITICAL: Adjust clusters to be unique within the line.
        //
        // Each segment is shaped independently with clusters starting from 0.
        // We must add the segment's byte position within line_text to make
        // clusters unique and correctly represent positions in the line.
        //
        // The 'start' value is the byte position of this segment within line_text.
        let cluster_offset = start as u32;

        // Debug logging for cluster adjustment verification
        if track_cluster && style_span.is_some() {
            log::debug!(
                "🔧 Styled segment: text='{}', start={}, cluster_offset={}, skip_spaces={}, leading_bytes={}",
                segment_text,
                start,
                cluster_offset,
                line.skip_leading_spaces,
                line.leading_space_bytes
            );
        }

        match self.shape_text_to_cells_with_offset(
            segment_text,
            &infos,
            font,
            context,
            style,
            track_cluster,
            cluster_offset,
        ) {
            Ok(cells) => Ok(cells),
            Err(e) => {
                // Check if this is an OutOfTextureSpace error that needs to propagate
                if e.root_cause().downcast_ref::<OutOfTextureSpace>().is_some() {
                    return Err(e);
                }

                // For other errors, try fallback logic
                log::error!(
                    "Failed to shape segment {:?} to cells: {}. Falling back to default font.",
                    segment_text,
                    e
                );
                // Fallback: try with default font
                if !Rc::ptr_eq(font, default_font) {
                    let window = self.window.as_ref().unwrap().clone();
                    let fallback_infos = default_font.shape(
                        segment_text,
                        move || window.notify(TermWindowNotif::InvalidateShapeCache),
                        BlockKey::filter_out_synthetic,
                        None,
                        wezterm_bidi::Direction::LeftToRight,
                        None,
                        None,
                    )?;
                    match self.shape_text_to_cells_with_offset(
                        segment_text,
                        &fallback_infos,
                        default_font,
                        context,
                        style,
                        track_cluster,
                        cluster_offset,
                    ) {
                        Ok(cells) => Ok(cells),
                        Err(e2) => {
                            // Also check for OutOfTextureSpace in fallback
                            if e2
                                .root_cause()
                                .downcast_ref::<OutOfTextureSpace>()
                                .is_some()
                            {
                                return Err(e2);
                            }
                            log::error!("Fallback also failed: {}", e2);
                            Ok(vec![]) // Return empty rather than fail the entire line
                        }
                    }
                } else {
                    Ok(vec![]) // Return empty rather than fail the entire line
                }
            }
        }
    }

    /// Shapes a wrapped line with the appropriate fonts based on style spans
    pub(crate) fn shape_line_with_styles(
        &self,
        line: &WrappedLine,
        style_spans: &[StyleSpan],
        default_font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &config::TextStyle,
        original_text: &str,
    ) -> anyhow::Result<Vec<ElementCell>> {
        let mut cells = Vec::new();

        // Extract the line text with proper offset handling
        let (full_line_text, line_text, local_byte_offset) = 
            self.extract_line_text_with_offsets(line, original_text, context)?;

        if line_text.is_empty() {
            return Ok(cells);
        }

        // Collect and process style spans for this line
        let segments = self.collect_style_spans_for_line(
            line,
            line_text,
            full_line_text,
            local_byte_offset,
            style_spans,
            context,
        );

        // Shape each segment with its appropriate font
        for (start, end, style_span) in segments {
            if start >= end {
                continue; // Skip empty segments
            }

            // Validate byte boundaries
            if !line_text.is_char_boundary(start) || !line_text.is_char_boundary(end) {
                log::error!(
                    "Invalid byte boundaries for segment: start={}, end={}, line_len={}. \
                     This indicates a bug in segment calculation.",
                    start,
                    end,
                    line_text.len()
                );
                // Skip this segment to avoid panic
                continue;
            }

            let segment_text = &line_text[start..end];
            let segment_cells = self.shape_segment_to_cells(
                segment_text,
                start,
                style_span,
                line,
                default_font,
                context,
                style,
            )?;
            cells.extend(segment_cells);
        }

        Ok(cells)
    }

    pub fn compute_element<'a>(
        &self,
        context: &LayoutContext,
        element: &Element,
    ) -> anyhow::Result<ComputedElement> {
        let local_metrics;
        let local_context;
        let context = if let Some(line_height) = element.line_height {
            local_metrics = context.metrics.scale_line_height(line_height);
            local_context = LayoutContext {
                height: DimensionContext {
                    dpi: context.height.dpi,
                    pixel_max: context.height.pixel_max,
                    pixel_cell: context.height.pixel_cell * line_height as f32,
                },
                width: context.width,
                bounds: context.bounds,
                gl_state: context.gl_state,
                metrics: &local_metrics,
                zindex: context.zindex,
                source: context.source,
            };
            &local_context
        } else {
            context
        };
        let border_corners = element
            .border_corners
            .as_ref()
            .map(|c| c.to_pixels(context));
        let style = element.font.style();
        let border = element.border.to_pixels(context);
        let padding = element.padding.to_pixels(context);
        let baseline = context.height.pixel_cell + context.metrics.descender.get() as f32;
        let min_width = match element.min_width {
            Some(w) => w.evaluate_as_pixels(context.width),
            None => 0.0,
        };
        let min_height = match element.min_height {
            Some(h) => h.evaluate_as_pixels(context.height),
            None => 0.0,
        };

        let border_and_padding_width = border.left + border.right + padding.left + padding.right;

        let max_width = match element.max_width {
            Some(w) => {
                w.evaluate_as_pixels(context.width)
                    .min(context.bounds.width())
                    - border_and_padding_width
            }
            None => context.bounds.width() - border_and_padding_width,
        }
        .min((context.width.pixel_max - context.bounds.min_x()) - border_and_padding_width);

        match &element.content {
            ElementContent::Text(s) => {
                let window = self.window.as_ref().unwrap().clone();
                let direction = wezterm_bidi::Direction::LeftToRight;
                let infos = element.font.shape(
                    &s,
                    move || window.notify(TermWindowNotif::InvalidateShapeCache),
                    BlockKey::filter_out_synthetic,
                    element.presentation,
                    direction,
                    None,
                    None,
                )?;
                let mut computed_cells = vec![];
                let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();
                let mut pixel_width = 0.0;
                let mut x_pos = context.bounds.min_x();
                let mut min_y = 0.0f32;
                // If element has no max_width constraint, use a very large value to shape all text
                let max_x = if element.max_width.is_none() {
                    f32::MAX
                } else {
                    context.bounds.min_x() + max_width
                };

                for info in infos {
                    let cell_start = &s[info.cluster as usize..];
                    let mut iter = Graphemes::new(cell_start).peekable();
                    let grapheme = iter
                        .next()
                        .ok_or_else(|| anyhow!("info.cluster didn't map into string"))?;
                    if let Some(key) = BlockKey::from_str(grapheme) {
                        // Only break if we have a max_width constraint
                        if element.max_width.is_some()
                            && pixel_width + context.width.pixel_cell >= max_x
                        {
                            break;
                        }
                        pixel_width += context.width.pixel_cell;
                        x_pos += context.width.pixel_cell;
                        let sprite = glyph_cache.cached_block(key, context.metrics)?;
                        computed_cells.push(ElementCell::Sprite(sprite));
                    } else {
                        let next_grapheme: Option<&str> = iter.peek().map(|s| *s);
                        let followed_by_space = next_grapheme == Some(" ");
                        let num_cells = grapheme_column_width(grapheme, None);
                        // Track clusters only for sidebar text
                        let track_cluster = context.source == RenderSource::Sidebar;
                        let glyph = glyph_cache.cached_glyph(
                            &info,
                            style,
                            followed_by_space,
                            &element.font,
                            context.metrics,
                            num_cells as u8,
                        )?;

                        if let Some(texture) = glyph.texture.as_ref() {
                            let x_pos = x_pos + (glyph.x_offset + glyph.bearing_x).get() as f32;
                            let width = texture.coords.size.width as f32 * glyph.scale as f32;
                            // Only break if we have a max_width constraint
                            if element.max_width.is_some() && x_pos + width >= max_x {
                                break;
                            }
                        } else if element.max_width.is_some()
                            && x_pos + glyph.x_advance.get() as f32 >= max_x
                        {
                            break;
                        }

                        min_y =
                            min_y.min(baseline - (glyph.y_offset + glyph.bearing_y).get() as f32);

                        pixel_width += glyph.x_advance.get() as f32;
                        x_pos += glyph.x_advance.get() as f32;

                        computed_cells.push(ElementCell::Glyph(glyph));
                    }
                }

                let content_rect = euclid::rect(
                    0.,
                    0.,
                    pixel_width.max(min_width),
                    context.height.pixel_cell.max(min_height),
                );

                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    semantic_type: element.semantic_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    layer_scissor: element.layer_scissor.clone(),
                    global_byte_offset: element.global_byte_offset,
                    content: ComputedElementContent::Text(computed_cells),
                })
            }
            ElementContent::WrappedText(text) => {
                // Determine if we should track clusters based on context
                let track_cluster = context.source == RenderSource::Sidebar;

                // Use wrap_text_with_info to get both cells and line information
                let (lines, wrapped_lines) = self.wrap_text_with_info(
                    text,
                    &element.font,
                    max_width,
                    context,
                    &style,
                    element.global_byte_offset,
                )?;
                let line_height = context.height.pixel_cell;
                let num_lines = lines.len() as f32;

                // Calculate max width of all lines for proper content rect
                let mut max_line_width: f32 = 0.0;
                for line in &lines {
                    let mut line_width = 0.0;
                    for cell in line {
                        line_width += self.get_cell_width(cell, context)?;
                    }
                    max_line_width = max_line_width.max(line_width);
                }

                let content_rect = euclid::rect(
                    0.,
                    0.,
                    max_line_width.max(min_width),
                    (line_height * num_lines).max(min_height),
                );

                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                // Create the computed element
                let mut computed = ComputedElement {
                    item_type: element.item_type.clone(),
                    semantic_type: element.semantic_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    layer_scissor: element.layer_scissor.clone(),
                    global_byte_offset: element.global_byte_offset,
                    content: ComputedElementContent::MultilineText {
                        lines: lines.clone(),
                        line_height,
                        line_styles: None,
                        line_font_styles: None,
                        line_info: Some(wrapped_lines.clone()),
                        line_positions: {
                            // Calculate actual Y positions for each line
                            // Use different spacing based on content type
                            let spacing_multiplier = if matches!(
                                element.item_type,
                                Some(UIItemType::ChatInput { .. })
                            ) {
                                1.1 // Chat input uses 1.1x spacing
                            } else {
                                1.0 // Regular content uses exact line height
                            };
                            let mut positions = Vec::new();
                            let mut y_pos = 0.0;
                            for _ in 0..lines.len() {
                                positions.push(y_pos);
                                y_pos += line_height * spacing_multiplier;
                            }
                            positions
                        },
                    },
                };

                // Extract exact glyph positions for chat input if clusters are tracked
                if track_cluster {
                    if let Some(UIItemType::ChatInput {
                        ref mut line_positions,
                    }) = computed.item_type
                    {
                        log::debug!("Extracting exact glyph positions for chat input text: {} wrapped lines, {} visual lines", 
                                   wrapped_lines.len(), lines.len());
                        // Clear any approximate positions that might have been set
                        line_positions.clear();

                        // Extract exact positions from shaped cells
                        for (idx, (cells, wrapped_line)) in
                            lines.iter().zip(wrapped_lines.iter()).enumerate()
                        {
                            let position_map = GlyphPositionMap::from_cells(cells, wrapped_line);
                            log::debug!(
                                "Line {}: extracted {} glyph positions from '{}' (bytes {}-{})",
                                idx,
                                position_map.positions.len(),
                                wrapped_line.shaped_text,
                                wrapped_line.byte_offset,
                                wrapped_line.byte_end
                            );
                            // Convert from (byte_offset, x_start, x_end) to (x_start, x_end, byte_offset)
                            let converted_positions: Vec<(f32, f32, usize)> = position_map
                                .positions
                                .iter()
                                .map(|&(byte_offset, x_start, x_end)| (x_start, x_end, byte_offset))
                                .collect();
                            line_positions.push(converted_positions);
                        }
                        log::debug!("Total lines with positions: {}", line_positions.len());
                    }
                }

                Ok(computed)
            }
            ElementContent::Children(kids) => {
                let mut block_pixel_width: f32 = 0.;
                let mut block_pixel_height: f32 = 0.;
                let mut computed_kids = vec![];
                let mut max_x: f32 = 0.;
                let mut float_width: f32 = 0.;
                let mut y_coord: f32 = 0.;

                for child in kids {
                    if child.display == DisplayType::Block {
                        y_coord += block_pixel_height;
                        block_pixel_height = 0.;
                        block_pixel_width = 0.;
                    }

                    let bounds = match child.float {
                        Float::None => euclid::rect(
                            block_pixel_width,
                            y_coord,
                            context.bounds.max_x() - (context.bounds.min_x() + block_pixel_width),
                            context.bounds.max_y() - (context.bounds.min_y() + y_coord),
                        ),
                        Float::Right => euclid::rect(
                            0.,
                            y_coord,
                            context.bounds.width(),
                            context.bounds.max_y() - (context.bounds.min_y() + y_coord),
                        ),
                    };
                    let kid = self.compute_element(
                        &LayoutContext {
                            bounds,
                            gl_state: context.gl_state,
                            height: context.height,
                            metrics: context.metrics,
                            width: DimensionContext {
                                dpi: context.width.dpi,
                                pixel_cell: context.width.pixel_cell,
                                pixel_max: max_width,
                            },
                            zindex: context.zindex + element.zindex,
                            source: context.source,
                        },
                        child,
                    )?;
                    match child.float {
                        Float::Right => {
                            float_width += float_width.max(kid.bounds.width());
                        }
                        Float::None => {
                            block_pixel_width += kid.bounds.width();
                            max_x = max_x.max(block_pixel_width);
                        }
                    }
                    block_pixel_height = block_pixel_height.max(kid.bounds.height());

                    computed_kids.push(kid);
                }

                // Respect min-width
                max_x = max_x.max(min_width);

                let mut float_max_x = (max_x + float_width).min(max_width);

                let pixel_height = (y_coord + block_pixel_height).max(min_height);

                for (kid, child) in computed_kids.iter_mut().zip(kids.iter()) {
                    match child.float {
                        Float::Right => {
                            max_x = max_x.max(float_max_x);
                            let x = float_max_x - kid.bounds.width();
                            float_max_x -= kid.bounds.width();
                            kid.translate(euclid::vec2(x, 0.));
                        }
                        _ => {}
                    }
                    match child.vertical_align {
                        VerticalAlign::Bottom => {
                            kid.translate(euclid::vec2(0., pixel_height - kid.bounds.height()));
                        }
                        VerticalAlign::Middle => {
                            kid.translate(euclid::vec2(
                                0.,
                                (pixel_height - kid.bounds.height()) / 2.0,
                            ));
                        }
                        VerticalAlign::Top => {}
                    }
                }

                computed_kids.sort_by(|a, b| a.zindex.cmp(&b.zindex));

                let content_rect = euclid::rect(0., 0., max_x.min(max_width), pixel_height);
                let rects = element.compute_rects(context, content_rect);

                for kid in &mut computed_kids {
                    kid.translate(rects.translate);
                }

                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    semantic_type: element.semantic_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    layer_scissor: element.layer_scissor.clone(),
                    global_byte_offset: element.global_byte_offset,
                    content: ComputedElementContent::Children(computed_kids),
                })
            }
            ElementContent::Poly { poly, line_width } => {
                let poly = poly.to_pixels(context);
                let content_rect = euclid::rect(0., 0., poly.width, poly.height.max(min_height));
                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    semantic_type: element.semantic_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    layer_scissor: element.layer_scissor.clone(),
                    global_byte_offset: element.global_byte_offset,
                    content: ComputedElementContent::Poly {
                        poly,
                        line_width: *line_width,
                    },
                })
            }
            ElementContent::StyledWrappedText { text, style_spans } => {
                // Use wrap_styled_text to get wrapped lines with style information
                let (lines, line_styles, line_font_styles, wrapped_lines) = self.wrap_styled_text(
                    text,
                    &element.font,
                    style_spans,
                    max_width,
                    context,
                    &style,
                    &element.colors,
                    element.global_byte_offset,
                )?;

                let line_height = context.height.pixel_cell;
                let num_lines = lines.len() as f32;

                // Calculate actual max width of all lines (like WrappedText does)
                let mut max_line_width: f32 = 0.0;
                for line in &lines {
                    let mut line_width = 0.0;
                    for cell in line {
                        line_width += self.get_cell_width(cell, context)?;
                    }
                    max_line_width = max_line_width.max(line_width);
                }

                let pixel_height = num_lines * line_height;
                let content_rect =
                    euclid::rect(0., 0., max_line_width.max(min_width), pixel_height);

                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                // Calculate line positions before moving lines
                let line_positions = {
                    // Use different spacing based on content type
                    let spacing_multiplier =
                        if matches!(element.item_type, Some(UIItemType::ChatInput { .. })) {
                            1.1 // Chat input uses 1.1x spacing
                        } else {
                            1.0 // Regular content uses exact line height
                        };
                    let mut positions = Vec::new();
                    let mut y_pos = 0.0;
                    for _ in 0..lines.len() {
                        positions.push(y_pos);
                        y_pos += line_height * spacing_multiplier;
                    }
                    positions
                };

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    semantic_type: element.semantic_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    layer_scissor: element.layer_scissor.clone(),
                    global_byte_offset: element.global_byte_offset,
                    content: ComputedElementContent::MultilineText {
                        lines,
                        line_height,
                        line_styles: Some(line_styles),
                        line_font_styles: Some(line_font_styles),
                        line_info: Some(wrapped_lines), // Now includes source text information for styled text
                        line_positions,
                    },
                })
            }
        }
    }

    pub fn render_element<'a>(
        &self,
        element: &ComputedElement,
        gl_state: &RenderState,
        inherited_colors: Option<&ElementColors>,
    ) -> anyhow::Result<()> {
        let layer = gl_state.layer_for_zindex(element.zindex)?;

        // If element contributes scissor, update layer
        if let Some(layer_scissor) = &element.layer_scissor {
            layer.update_scissor_rect(layer_scissor.rect);
        }

        let mut layers = layer.quad_allocator();

        let colors = match &element.hover_colors {
            Some(hc) => {
                let hovering =
                    match &self.current_mouse_event {
                        Some(event) => {
                            let mouse_x = event.coords.x as f32;
                            let mouse_y = event.coords.y as f32;
                            mouse_x >= element.bounds.min_x()
                                && mouse_x <= element.bounds.max_x()
                                && mouse_y >= element.bounds.min_y()
                                && mouse_y <= element.bounds.max_y()
                        }
                        None => false,
                    } && matches!(self.current_mouse_capture, None | Some(MouseCapture::UI));
                if hovering {
                    hc
                } else {
                    &element.colors
                }
            }
            None => &element.colors,
        };

        self.render_element_background(element, colors, &mut layers, inherited_colors)?;

        let left = self.dimensions.pixel_width as f32 / -2.0;
        let top = self.dimensions.pixel_height as f32 / -2.0;
        match &element.content {
            ComputedElementContent::Text(cells) => {
                let mut pos_x = element.content_rect.min_x();
                // Check if we should apply manual clipping based on clip_bounds
                let should_clip = element.clip_bounds.is_some();
                let clip_min_x = element
                    .clip_bounds
                    .as_ref()
                    .map(|b| b.min_x())
                    .unwrap_or(f32::MIN);
                let clip_max_x = element
                    .clip_bounds
                    .as_ref()
                    .map(|b| b.max_x())
                    .unwrap_or(f32::MAX);

                for cell in cells {
                    // No clipping - render all content
                    match cell {
                        ElementCell::Sprite(sprite) => {
                            let width = sprite.coords.width();
                            let height = sprite.coords.height();
                            let pos_y = top + element.content_rect.min_y();

                            // Manual clipping check
                            if should_clip {
                                // Apply the same left offset used in rendering to get actual screen coordinates
                                let sprite_left = pos_x + left;
                                let sprite_right = pos_x + left + width as f32;

                                // Skip sprites entirely outside clip bounds
                                if sprite_right < clip_min_x || sprite_left > clip_max_x {
                                    log::trace!("Skipping sprite outside clip bounds: sprite [{}, {}], clip [{}, {}]", 
                                        sprite_left, sprite_right, clip_min_x, clip_max_x);
                                    pos_x += width as f32;
                                    continue;
                                }

                                // Handle partially clipped sprites
                                if sprite_left < clip_min_x || sprite_right > clip_max_x {
                                    // Calculate visible portion
                                    let visible_left = sprite_left.max(clip_min_x);
                                    let visible_right = sprite_right.min(clip_max_x);
                                    let visible_width = visible_right - visible_left;

                                    // Calculate texture coordinate adjustments
                                    let sprite_width = width as f32;
                                    let left_clip_ratio =
                                        (visible_left - sprite_left) / sprite_width;
                                    let right_clip_ratio =
                                        (sprite_right - visible_right) / sprite_width;

                                    log::trace!("Partial sprite clipping: sprite [{}, {}], visible [{}, {}], clip ratios: left={:.3}, right={:.3}", 
                                        sprite_left, sprite_right, visible_left, visible_right, left_clip_ratio, right_clip_ratio);

                                    // Adjust texture coordinates
                                    let tex_coords = sprite.texture_coords();
                                    let tex_width = tex_coords.max_x() - tex_coords.min_x();
                                    let adjusted_tex_left =
                                        tex_coords.min_x() + left_clip_ratio * tex_width;
                                    let adjusted_tex_right =
                                        tex_coords.max_x() - right_clip_ratio * tex_width;

                                    // Create quad with adjusted position and texture
                                    let mut quad = layers.allocate(2)?;
                                    quad.set_position(
                                        visible_left,
                                        pos_y,
                                        visible_right,
                                        pos_y + height as f32,
                                    );
                                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                    quad.set_texture(euclid::rect(
                                        adjusted_tex_left,
                                        tex_coords.min_y(),
                                        adjusted_tex_right - adjusted_tex_left,
                                        tex_coords.max_y() - tex_coords.min_y(),
                                    ));
                                    quad.set_hsv(None);

                                    pos_x += width as f32;
                                    continue;
                                }
                            }

                            let mut quad = layers.allocate(2)?;
                            quad.set_position(
                                pos_x + left,
                                pos_y,
                                pos_x + left + width as f32,
                                pos_y + height as f32,
                            );
                            self.resolve_text(colors, inherited_colors).apply(&mut quad);
                            quad.set_texture(sprite.texture_coords());
                            quad.set_hsv(None);
                            pos_x += width as f32;
                        }
                        ElementCell::Glyph(glyph) | ElementCell::GlyphWithCluster { glyph, .. } => {
                            if let Some(texture) = glyph.texture.as_ref() {
                                let pos_y = element.content_rect.min_y() as f32 + top
                                    - (glyph.y_offset + glyph.bearing_y).get() as f32
                                    + element.baseline;

                                // Don't break early if we have clip bounds
                                if !should_clip
                                    && pos_x + glyph.x_advance.get() as f32
                                        > element.content_rect.max_x()
                                {
                                    break;
                                }
                                let glyph_pos_x =
                                    pos_x + (glyph.x_offset + glyph.bearing_x).get() as f32;
                                let width = texture.coords.size.width as f32 * glyph.scale as f32;
                                let height = texture.coords.size.height as f32 * glyph.scale as f32;

                                // Manual clipping check for glyphs
                                if should_clip {
                                    // Apply the same left offset used in rendering to get actual screen coordinates
                                    let glyph_left = glyph_pos_x + left;
                                    let glyph_right = glyph_pos_x + left + width;

                                    // Skip glyphs entirely outside clip bounds
                                    if glyph_right < clip_min_x || glyph_left > clip_max_x {
                                        log::trace!("Skipping glyph outside clip bounds: glyph [{}, {}], clip [{}, {}]", 
                                            glyph_left, glyph_right, clip_min_x, clip_max_x);
                                        pos_x += glyph.x_advance.get() as f32;
                                        continue;
                                    }

                                    // Handle partially clipped glyphs
                                    if glyph_left < clip_min_x || glyph_right > clip_max_x {
                                        // Calculate visible portion
                                        let visible_left = glyph_left.max(clip_min_x);
                                        let visible_right = glyph_right.min(clip_max_x);
                                        let visible_width = visible_right - visible_left;

                                        // Calculate texture coordinate adjustments
                                        let glyph_width = width;
                                        let left_clip_ratio =
                                            (visible_left - glyph_left) / glyph_width;
                                        let right_clip_ratio =
                                            (glyph_right - visible_right) / glyph_width;

                                        log::trace!("Partial glyph clipping: glyph [{}, {}], visible [{}, {}], clip ratios: left={:.3}, right={:.3}", 
                                            glyph_left, glyph_right, visible_left, visible_right, left_clip_ratio, right_clip_ratio);

                                        // Adjust texture coordinates
                                        let tex_coords = texture.texture_coords();
                                        let tex_width = tex_coords.max_x() - tex_coords.min_x();
                                        let adjusted_tex_left =
                                            tex_coords.min_x() + left_clip_ratio * tex_width;
                                        let adjusted_tex_right =
                                            tex_coords.max_x() - right_clip_ratio * tex_width;

                                        // Create quad with adjusted position and texture
                                        let mut quad = layers.allocate(1)?;
                                        quad.set_position(
                                            visible_left,
                                            pos_y,
                                            visible_right,
                                            pos_y + height,
                                        );
                                        self.resolve_text(colors, inherited_colors)
                                            .apply(&mut quad);
                                        quad.set_texture(euclid::rect(
                                            adjusted_tex_left,
                                            tex_coords.min_y(),
                                            adjusted_tex_right - adjusted_tex_left,
                                            tex_coords.max_y() - tex_coords.min_y(),
                                        ));
                                        quad.set_hsv(None);
                                        quad.set_has_color(glyph.has_color);

                                        pos_x += glyph.x_advance.get() as f32;
                                        continue;
                                    }
                                }

                                let mut quad = layers.allocate(1)?;
                                quad.set_position(
                                    glyph_pos_x + left,
                                    pos_y,
                                    glyph_pos_x + left + width,
                                    pos_y + height,
                                );
                                self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                quad.set_texture(texture.texture_coords());
                                quad.set_has_color(glyph.has_color);
                                quad.set_hsv(None);
                            }
                            pos_x += glyph.x_advance.get() as f32;
                        }
                    }
                }
            }
            ComputedElementContent::MultilineText {
                lines,
                line_height,
                line_styles,
                line_font_styles,
                line_info: _,
                line_positions,
            } => {
                let mut y_offset = 0.0;

                // Use segment batching if we have per-cell styles
                if let Some(ref styles) = line_styles {
                    // Render with segment batching for better performance
                    for (line_idx, (line_cells, line_colors)) in
                        lines.iter().zip(styles.iter()).enumerate()
                    {
                        // Use actual Y position from line_positions
                        let y = element.content_rect.min_y()
                            + line_positions
                                .get(line_idx)
                                .copied()
                                .unwrap_or(line_idx as f32 * line_height);

                        // Group consecutive cells with the same color
                        let mut segment_start = 0;
                        while segment_start < line_cells.len() {
                            let start_color = &line_colors[segment_start];
                            let mut segment_end = segment_start + 1;

                            // Find end of this color segment
                            while segment_end < line_cells.len()
                                && line_colors[segment_end] == *start_color
                            {
                                segment_end += 1;
                            }

                            // Render this segment with the same color
                            let mut pos_x = element.content_rect.min_x();
                            // Skip to segment start position
                            for i in 0..segment_start {
                                match &line_cells[i] {
                                    ElementCell::Sprite(sprite) => {
                                        pos_x += sprite.coords.width() as f32
                                    }
                                    ElementCell::Glyph(glyph)
                                    | ElementCell::GlyphWithCluster { glyph, .. } => {
                                        pos_x += glyph.x_advance.get() as f32
                                    }
                                }
                            }

                            // Render the segment
                            // No clipping - allow text to render wherever it needs to
                            for cell_idx in segment_start..segment_end {
                                match &line_cells[cell_idx] {
                                    ElementCell::Sprite(sprite) => {
                                        let width = sprite.coords.width();
                                        let height = sprite.coords.height();
                                        let pos_y = top + y;

                                        let mut quad = layers.allocate(2)?;
                                        quad.set_position(
                                            pos_x + left,
                                            pos_y,
                                            pos_x + left + width as f32,
                                            pos_y + height as f32,
                                        );
                                        self.resolve_text(start_color, inherited_colors)
                                            .apply(&mut quad);
                                        quad.set_texture(sprite.texture_coords());
                                        quad.set_hsv(None);
                                        pos_x += width as f32;
                                    }
                                    ElementCell::Glyph(glyph)
                                    | ElementCell::GlyphWithCluster { glyph, .. } => {
                                        if let Some(texture) = glyph.texture.as_ref() {
                                            let pos_y = y as f32 + top
                                                - (glyph.y_offset + glyph.bearing_y).get() as f32
                                                + element.baseline;

                                            // Allow glyphs to render even if they extend past content boundaries
                                            // The wrap calculation should handle preventing overflow
                                            // We explicitly do NOT clip here because we want characters to be
                                            // visible even if they extend slightly into padding areas.
                                            // Clipping was causing character loss at wrap boundaries.
                                            let glyph_x = pos_x
                                                + (glyph.x_offset + glyph.bearing_x).get() as f32;
                                            let width = texture.coords.size.width as f32
                                                * glyph.scale as f32;
                                            let height = texture.coords.size.height as f32
                                                * glyph.scale as f32;

                                            let mut quad = layers.allocate(1)?;
                                            quad.set_position(
                                                glyph_x + left,
                                                pos_y,
                                                glyph_x + left + width,
                                                pos_y + height,
                                            );
                                            self.resolve_text(start_color, inherited_colors)
                                                .apply(&mut quad);
                                            quad.set_texture(texture.texture_coords());
                                            quad.set_has_color(glyph.has_color);
                                            quad.set_hsv(None);
                                        }
                                        pos_x += glyph.x_advance.get() as f32;
                                    }
                                }
                            }

                            segment_start = segment_end;
                        }
                    }
                } else {
                    // Original rendering without per-cell styles
                    // No clipping - allow text to render wherever it needs to
                    for (line_idx, line_cells) in lines.iter().enumerate() {
                        let mut pos_x = element.content_rect.min_x();
                        // Use actual Y position from line_positions
                        let y = element.content_rect.min_y()
                            + line_positions
                                .get(line_idx)
                                .copied()
                                .unwrap_or(line_idx as f32 * line_height);

                        for cell in line_cells.iter() {
                            match cell {
                                ElementCell::Sprite(sprite) => {
                                    let width = sprite.coords.width();
                                    let height = sprite.coords.height();
                                    let pos_y = top + y;

                                    let mut quad = layers.allocate(2)?;
                                    quad.set_position(
                                        pos_x + left,
                                        pos_y,
                                        pos_x + left + width as f32,
                                        pos_y + height as f32,
                                    );
                                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                    quad.set_texture(sprite.texture_coords());
                                    quad.set_hsv(None);
                                    pos_x += width as f32;
                                }
                                ElementCell::Glyph(glyph)
                                | ElementCell::GlyphWithCluster { glyph, .. } => {
                                    if let Some(texture) = glyph.texture.as_ref() {
                                        let pos_y = y as f32 + top
                                            - (glyph.y_offset + glyph.bearing_y).get() as f32
                                            + element.baseline;

                                        // No clipping check - render the glyph regardless
                                        let pos_x =
                                            pos_x + (glyph.x_offset + glyph.bearing_x).get() as f32;
                                        let width =
                                            texture.coords.size.width as f32 * glyph.scale as f32;
                                        let height =
                                            texture.coords.size.height as f32 * glyph.scale as f32;

                                        let mut quad = layers.allocate(1)?;
                                        quad.set_position(
                                            pos_x + left,
                                            pos_y,
                                            pos_x + left + width,
                                            pos_y + height,
                                        );
                                        self.resolve_text(colors, inherited_colors)
                                            .apply(&mut quad);
                                        quad.set_texture(texture.texture_coords());
                                        quad.set_has_color(glyph.has_color);
                                        quad.set_hsv(None);
                                    }
                                    pos_x += glyph.x_advance.get() as f32;
                                }
                            }
                        }
                    }
                }
            }
            ComputedElementContent::Children(kids) => {
                drop(layers);

                for kid in kids {
                    self.render_element(kid, gl_state, Some(colors))?;
                }
            }
            ComputedElementContent::Poly { poly, line_width } => {
                if element.content_rect.width() >= poly.width {
                    let mut quad = self.poly_quad(
                        &mut layers,
                        1,
                        element.content_rect.origin,
                        poly.poly,
                        *line_width,
                        euclid::size2(poly.width, poly.height),
                        LinearRgba::TRANSPARENT,
                    )?;
                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                }
            }
        }

        Ok(())
    }

    fn resolve_text(
        &self,
        colors: &ElementColors,
        inherited_colors: Option<&ElementColors>,
    ) -> ResolvedColor {
        match &colors.text {
            InheritableColor::Inherited => match inherited_colors {
                Some(colors) => self.resolve_text(colors, None),
                None => LinearRgba::TRANSPARENT.into(),
            },
            InheritableColor::Color(color) => {
                // Check if the color has non-standard alpha (not fully opaque)
                let mut resolved = ResolvedColor::from(*color);
                if color.3 < 1.0 {
                    // Preserve the alpha from the original color
                    resolved.alpha_override = Some(color.3);
                }
                resolved
            }
            InheritableColor::Animated {
                color,
                alt_color,
                ease,
                one_shot,
            } => {
                if let Some((mix_value, next)) = ease.borrow_mut().intensity(*one_shot) {
                    self.update_next_frame_time(Some(next));
                    let mut resolved = ResolvedColor {
                        color: *color,
                        alt_color: *alt_color,
                        mix_value,
                        alpha_override: None,
                    };
                    // Check if either color has non-standard alpha
                    if color.3 < 1.0 || alt_color.3 < 1.0 {
                        // For animated colors, we might need to interpolate alpha
                        // For now, just use the primary color's alpha
                        resolved.alpha_override = Some(color.3);
                    }
                    resolved
                } else {
                    let mut resolved = ResolvedColor::from(*color);
                    if color.3 < 1.0 {
                        resolved.alpha_override = Some(color.3);
                    }
                    resolved
                }
            }
        }
    }

    fn resolve_bg(
        &self,
        colors: &ElementColors,
        inherited_colors: Option<&ElementColors>,
    ) -> ResolvedColor {
        match &colors.bg {
            InheritableColor::Inherited => match inherited_colors {
                Some(colors) => self.resolve_bg(colors, None),
                None => LinearRgba::TRANSPARENT.into(),
            },
            InheritableColor::Color(color) => (*color).into(),
            InheritableColor::Animated {
                color,
                alt_color,
                ease,
                one_shot,
            } => {
                if let Some((mix_value, next)) = ease.borrow_mut().intensity(*one_shot) {
                    self.update_next_frame_time(Some(next));
                    ResolvedColor {
                        color: *color,
                        alt_color: *alt_color,
                        mix_value,
                        alpha_override: None,
                    }
                } else {
                    (*color).into()
                }
            }
        }
    }

    fn render_element_background<'a>(
        &self,
        element: &ComputedElement,
        colors: &ElementColors,
        layers: &mut TripleLayerQuadAllocator,
        inherited_colors: Option<&ElementColors>,
    ) -> anyhow::Result<()> {
        let mut top_left_width = 0.;
        let mut top_left_height = 0.;
        let mut top_right_width = 0.;
        let mut top_right_height = 0.;

        let mut bottom_left_width = 0.;
        let mut bottom_left_height = 0.;
        let mut bottom_right_width = 0.;
        let mut bottom_right_height = 0.;

        if let Some(c) = &element.border_corners {
            top_left_width = c.top_left.width;
            top_left_height = c.top_left.height;
            top_right_width = c.top_right.width;
            top_right_height = c.top_right.height;

            bottom_left_width = c.bottom_left.width;
            bottom_left_height = c.bottom_left.height;
            bottom_right_width = c.bottom_right.width;
            bottom_right_height = c.bottom_right.height;

            if top_left_width > 0. && top_left_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    element.border_rect.origin,
                    c.top_left.poly,
                    element.border.top as isize,
                    euclid::size2(top_left_width, top_left_height),
                    colors.border.top,
                )?
                .set_grayscale();
            }
            if top_right_width > 0. && top_right_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    euclid::point2(
                        element.border_rect.max_x() - top_right_width,
                        element.border_rect.min_y(),
                    ),
                    c.top_right.poly,
                    element.border.top as isize,
                    euclid::size2(top_right_width, top_right_height),
                    colors.border.top,
                )?
                .set_grayscale();
            }
            if bottom_left_width > 0. && bottom_left_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    euclid::point2(
                        element.border_rect.min_x(),
                        element.border_rect.max_y() - bottom_left_height,
                    ),
                    c.bottom_left.poly,
                    element.border.bottom as isize,
                    euclid::size2(bottom_left_width, bottom_left_height),
                    colors.border.bottom,
                )?
                .set_grayscale();
            }
            if bottom_right_width > 0. && bottom_right_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    euclid::point2(
                        element.border_rect.max_x() - bottom_right_width,
                        element.border_rect.max_y() - bottom_right_height,
                    ),
                    c.bottom_right.poly,
                    element.border.bottom as isize,
                    euclid::size2(bottom_right_width, bottom_right_height),
                    colors.border.bottom,
                )?
                .set_grayscale();
            }

            // Filling the background is more complex because we can't
            // simply fill the padding rect--we'd clobber the corner
            // graphics.
            // Instead, we consider the element as consisting of:
            //
            //   TL T TR
            //   L  C  R
            //   BL B BR
            //
            // We already rendered the corner pieces, so now we need
            // to do the rest

            // The `T` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + top_left_width,
                    element.border_rect.min_y(),
                    element.border_rect.width() - (top_left_width + top_right_width) as f32,
                    top_left_height.max(top_right_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `B` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + bottom_left_width,
                    element.border_rect.max_y() - bottom_left_height.max(bottom_right_height),
                    element.border_rect.width() - (bottom_left_width + bottom_right_width),
                    bottom_left_height.max(bottom_right_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `L` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x(),
                    element.border_rect.min_y() + top_left_height,
                    top_left_width.max(bottom_left_width),
                    element.border_rect.height() - (top_left_height + bottom_left_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `R` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.max_x() - top_right_width,
                    element.border_rect.min_y() + top_right_height,
                    top_right_width.max(bottom_right_width),
                    element.border_rect.height() - (top_right_height + bottom_right_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `C` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + top_left_width,
                    element.border_rect.min_y() + top_right_height.min(top_left_height),
                    element.border_rect.width() - (top_left_width + top_right_width),
                    element.border_rect.height()
                        - (top_right_height.min(top_left_height)
                            + bottom_right_height.min(bottom_left_height)),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);
        } else {
            let resolved_bg = self.resolve_bg(colors, inherited_colors);
            if resolved_bg.color != LinearRgba::TRANSPARENT {
                let mut quad =
                    self.filled_rectangle(layers, 0, element.padding, LinearRgba::TRANSPARENT)?;
                resolved_bg.apply(&mut quad);
            }
        }

        if element.border_rect == element.padding {
            // There's no border to be drawn
            return Ok(());
        }

        if element.border.top > 0. && colors.border.top != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + top_left_width as f32,
                    element.border_rect.min_y(),
                    element.border_rect.width() - (top_left_width + top_right_width) as f32,
                    element.border.top,
                ),
                colors.border.top,
            )?;
        }
        if element.border.bottom > 0. && colors.border.bottom != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + bottom_left_width as f32,
                    element.border_rect.max_y() - element.border.bottom,
                    element.border_rect.width() - (bottom_left_width + bottom_right_width) as f32,
                    element.border.bottom,
                ),
                colors.border.bottom,
            )?;
        }
        if element.border.left > 0. && colors.border.left != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x(),
                    element.border_rect.min_y() + top_left_height as f32,
                    element.border.left,
                    element.border_rect.height() - (top_left_height + bottom_left_height) as f32,
                ),
                colors.border.left,
            )?;
        }
        if element.border.right > 0. && colors.border.right != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.max_x() - element.border.right,
                    element.border_rect.min_y() + top_right_height as f32,
                    element.border.left,
                    element.border_rect.height() - (top_right_height + bottom_right_height) as f32,
                ),
                colors.border.right,
            )?;
        }

        Ok(())
    }
}
