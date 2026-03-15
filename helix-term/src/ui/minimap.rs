use helix_view::{graphics::Rect, graphics::Style, view::MinimapCache, DocumentId};
use tui::buffer::Buffer as Surface;

const BRAILLE_BASE: u32 = 0x2800;
// Braille dot positions:
// 0 3
// 1 4
// 2 5
// 6 7
const DOT_BITS: [u8; 8] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80];

pub fn render(
    text: &helix_core::Rope,
    doc_id: DocumentId,
    first_visible_line: usize,
    inner_height: usize,
    text_style: Style,
    viewport_style: Style,
    separator_style: Style,
    cache: &mut MinimapCache,
    area: Rect,
    surface: &mut Surface,
) {
    let total_lines = text.len_lines();
    let minimap_height = area.height as usize;
    let minimap_width = area.width as usize;

    if total_lines == 0 || minimap_height == 0 || minimap_width == 0 {
        return;
    }

    // Check if cache is valid
    let cache_valid = cache.doc_id == Some(doc_id)
        && cache.line_count == total_lines
        && cache.width == minimap_width
        && cache.height == minimap_height;

    if !cache_valid {
        // Recompute cache
        cache.chars = compute_minimap_chars(text, minimap_width, minimap_height, total_lines);
        cache.doc_id = Some(doc_id);
        cache.line_count = total_lines;
        cache.width = minimap_width;
        cache.height = minimap_height;
    }

    // Each braille char = 4 lines vertically
    let total_rows = minimap_height * 4;

    // Viewport bounds
    let last_visible = (first_visible_line + inner_height.saturating_sub(2)).min(total_lines);

    // Scale entire file to fit minimap
    let lines_per_row = total_lines as f32 / total_rows as f32;

    // Viewport highlight bounds (in minimap row coordinates)
    let vp_start_row = (first_visible_line as f32 / lines_per_row) as usize;
    let vp_end_row = ((last_visible as f32 / lines_per_row) as usize + 1).min(total_rows);

    // Draw from cache with viewport highlight
    for y in 0..minimap_height {
        let row = area.y + y as u16;

        // Check if any part of this char row is in viewport
        let row_start = y * 4;
        let row_end = row_start + 4;
        let in_viewport = row_start < vp_end_row && row_end > vp_start_row;
        let style = if in_viewport { viewport_style } else { text_style };

        // Highlight entire row for viewport
        if in_viewport {
            for x in 0..minimap_width {
                let cell = &mut surface[(area.x + x as u16, row)];
                cell.set_style(viewport_style);
            }
        }

        // Draw cached braille characters
        for x in 0..minimap_width {
            let ch = cache.chars[y][x];
            if ch != ' ' {
                let cell = &mut surface[(area.x + x as u16, row)];
                cell.set_char(ch);
                cell.set_style(style);
            }
        }

        // Separator
        if area.x > 0 {
            let cell = &mut surface[(area.x - 1, row)];
            cell.set_char('│');
            cell.set_style(separator_style);
        }
    }
}

/// Compute the braille character grid for the minimap
fn compute_minimap_chars(
    text: &helix_core::Rope,
    minimap_width: usize,
    minimap_height: usize,
    total_lines: usize,
) -> Vec<Vec<char>> {
    let total_rows = minimap_height * 4;
    let lines_per_row = total_lines as f32 / total_rows as f32;
    let cols_per_dot = (120 / (minimap_width * 2)).max(1);

    let mut chars = vec![vec![' '; minimap_width]; minimap_height];

    for y in 0..minimap_height {
        for x in 0..minimap_width {
            let mut dots: u8 = 0;

            // Check each of the 8 dot positions
            for dot in 0..8 {
                let dot_row = match dot {
                    0 | 3 => 0,
                    1 | 4 => 1,
                    2 | 5 => 2,
                    6 | 7 => 3,
                    _ => unreachable!(),
                };
                let dot_col = match dot {
                    0 | 1 | 2 | 6 => 0,
                    3 | 4 | 5 | 7 => 1,
                    _ => unreachable!(),
                };

                // Calculate which document lines map to this minimap row position
                let minimap_row = y * 4 + dot_row;
                let doc_line_start = (minimap_row as f32 * lines_per_row) as usize;
                let doc_line_end =
                    (((minimap_row + 1) as f32 * lines_per_row) as usize).max(doc_line_start + 1);

                let col_start = (x * 2 + dot_col) * cols_per_dot;

                // Check all document lines that map to this dot position
                let mut has_content = false;
                for line_idx in doc_line_start..doc_line_end.min(total_lines) {
                    let line = text.line(line_idx);
                    let line_len = line.len_bytes().saturating_sub(1);

                    // Check if there's content at this position
                    if line_len > col_start {
                        let col_end = (col_start + cols_per_dot).min(line_len);

                        let mut pos = 0;
                        'chunk: for chunk in line.chunks() {
                            let bytes = chunk.as_bytes();
                            for &b in bytes {
                                if pos >= col_end {
                                    break 'chunk;
                                }
                                if pos >= col_start && b > b' ' && b != b'\n' && b != b'\r' {
                                    has_content = true;
                                    break 'chunk;
                                }
                                pos += 1;
                            }
                        }

                        if has_content {
                            break;
                        }
                    }
                }

                if has_content {
                    dots |= DOT_BITS[dot];
                }
            }

            if dots != 0 {
                chars[y][x] = char::from_u32(BRAILLE_BASE + dots as u32).unwrap_or(' ');
            }
        }
    }

    chars
}
