pub fn text_icon(text: &str, glyph_color: [u8; 4]) -> ksni::Icon {
    const SIZE: usize = 32;
    const DIGIT_WIDTH: usize = 3;
    const DIGIT_HEIGHT: usize = 5;
    const PADDING: usize = 1;
    const DIGIT_GAP: usize = 2;

    let digit_count = text.len();
    let total_gap_width = digit_count.saturating_sub(1) * DIGIT_GAP;
    let available_width = SIZE
        .saturating_sub(PADDING * 2 + total_gap_width)
        .max(DIGIT_WIDTH);
    let available_height = SIZE.saturating_sub(PADDING * 2).max(DIGIT_HEIGHT);
    let digit_slot_width = (available_width / digit_count.max(1)).max(DIGIT_WIDTH);
    let digit_column_widths = dimension_slices::<DIGIT_WIDTH>(digit_slot_width);
    let digit_row_heights = dimension_slices::<DIGIT_HEIGHT>(available_height);
    let text_width = digit_count * digit_slot_width + total_gap_width;
    let text_height = digit_row_heights.iter().sum::<usize>();
    let offset_x = (SIZE - text_width) / 2;
    let offset_y = (SIZE - text_height) / 2;

    let mut rgba = vec![0_u8; SIZE * SIZE * 4];

    for (index, ch) in text.chars().enumerate() {
        let digit_x = offset_x + index * (digit_slot_width + DIGIT_GAP);
        draw_digit(
            &mut rgba,
            ch,
            GlyphLayout {
                canvas_width: SIZE,
                start_x: digit_x,
                start_y: offset_y,
                column_widths: &digit_column_widths,
                row_heights: &digit_row_heights,
                color: glyph_color,
            },
        );
    }

    rgba_to_argb(&mut rgba);

    ksni::Icon {
        width: SIZE as i32,
        height: SIZE as i32,
        data: rgba,
    }
}

fn fill_rect(
    rgba: &mut [u8],
    canvas_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    for row in y..(y + height) {
        for column in x..(x + width) {
            let pixel = (row * canvas_width + column) * 4;
            rgba[pixel..pixel + 4].copy_from_slice(&color);
        }
    }
}

struct GlyphLayout<'a> {
    canvas_width: usize,
    start_x: usize,
    start_y: usize,
    column_widths: &'a [usize; 3],
    row_heights: &'a [usize; 5],
    color: [u8; 4],
}

fn draw_digit(rgba: &mut [u8], ch: char, layout: GlyphLayout<'_>) {
    let Some(pattern) = digit_pattern(ch) else {
        return;
    };

    let mut y = layout.start_y;
    for (row, row_pattern) in pattern.iter().enumerate() {
        let mut x = layout.start_x;
        for (column, pixel) in row_pattern.chars().enumerate() {
            if pixel == '1' {
                fill_rect(
                    rgba,
                    layout.canvas_width,
                    x,
                    y,
                    layout.column_widths[column],
                    layout.row_heights[row],
                    layout.color,
                );
            }

            x += layout.column_widths[column];
        }

        y += layout.row_heights[row];
    }
}

fn dimension_slices<const COUNT: usize>(total: usize) -> [usize; COUNT] {
    let base = total / COUNT;
    let remainder = total % COUNT;
    let middle = COUNT / 2;
    let mut slices = [base; COUNT];

    for offset in 0..remainder {
        let index = if offset % 2 == 0 {
            middle.saturating_sub(offset / 2)
        } else {
            (middle + 1 + offset / 2).min(COUNT - 1)
        };
        slices[index] += 1;
    }

    slices
}

fn digit_pattern(ch: char) -> Option<[&'static str; 5]> {
    match ch {
        '0' => Some(["111", "101", "101", "101", "111"]),
        '1' => Some(["010", "110", "010", "010", "111"]),
        '2' => Some(["111", "001", "111", "100", "111"]),
        '3' => Some(["111", "001", "111", "001", "111"]),
        '4' => Some(["101", "101", "111", "001", "001"]),
        '5' => Some(["111", "100", "111", "001", "111"]),
        '6' => Some(["111", "100", "111", "101", "111"]),
        '7' => Some(["111", "001", "001", "001", "001"]),
        '8' => Some(["111", "101", "111", "101", "111"]),
        '9' => Some(["111", "101", "111", "001", "111"]),
        '-' => Some(["000", "000", "111", "000", "000"]),
        _ => None,
    }
}

fn rgba_to_argb(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
}
