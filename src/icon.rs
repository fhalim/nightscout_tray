pub fn numeric_icon(reading: u16) -> ksni::Icon {
    const SIZE: usize = 32;
    const SCALE: usize = 2;
    const DIGIT_WIDTH: usize = 3;
    const DIGIT_HEIGHT: usize = 5;
    const DIGIT_SPACING: usize = 1;
    const DIGIT_COLOR: [u8; 4] = [32, 122, 74, 255];

    let text = reading.to_string();
    let digit_count = text.len();
    let text_width =
        digit_count * DIGIT_WIDTH * SCALE + digit_count.saturating_sub(1) * DIGIT_SPACING * SCALE;
    let text_height = DIGIT_HEIGHT * SCALE;
    let offset_x = (SIZE - text_width) / 2;
    let offset_y = (SIZE - text_height) / 2;

    let mut rgba = vec![0_u8; SIZE * SIZE * 4];

    for (index, ch) in text.chars().enumerate() {
        let digit_x = offset_x + index * (DIGIT_WIDTH + DIGIT_SPACING) * SCALE;
        draw_digit(&mut rgba, SIZE, ch, digit_x, offset_y, SCALE, DIGIT_COLOR);
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

fn draw_digit(
    rgba: &mut [u8],
    canvas_width: usize,
    ch: char,
    start_x: usize,
    start_y: usize,
    scale: usize,
    color: [u8; 4],
) {
    let Some(pattern) = digit_pattern(ch) else {
        return;
    };

    for (row, row_pattern) in pattern.iter().enumerate() {
        for (column, pixel) in row_pattern.chars().enumerate() {
            if pixel == '1' {
                fill_rect(
                    rgba,
                    canvas_width,
                    start_x + column * scale,
                    start_y + row * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
    }
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
        _ => None,
    }
}

fn rgba_to_argb(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
}
