//! Source-position helpers shared by consumers that speak UTF-16.

/// Converts a 0-based byte column within a single line to a 0-based UTF-16 code-unit
/// column.
///
/// tree-sitter reports byte columns while LSP and JSON editors count UTF-16 code units,
/// so every non-ASCII character before the position shifts the result. An out-of-range
/// `byte_col` is clamped to the line length, and a column that lands inside a multi-byte
/// character is clamped back to that character's start so no code point is split.
pub fn byte_col_to_utf16(line: &str, byte_col: usize) -> usize {
    let boundary = floor_char_boundary(line, byte_col);
    line[..boundary].chars().map(char::len_utf16).sum()
}

/// Largest character boundary at or before `byte_col`, clamped to `line`'s length.
fn floor_char_boundary(line: &str, byte_col: usize) -> usize {
    let mut boundary = byte_col.min(line.len());
    while !line.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_keeps_the_same_column() {
        assert_eq!(byte_col_to_utf16("const x = 1;", 0), 0);
        assert_eq!(byte_col_to_utf16("const x = 1;", 6), 6);
        assert_eq!(byte_col_to_utf16("const x = 1;", 12), 12);
    }

    #[test]
    fn accented_characters_count_one_code_unit() {
        // "ñ" is two UTF-8 bytes but one UTF-16 code unit.
        assert_eq!(byte_col_to_utf16("const ñ = 1;", 6), 6);
        assert_eq!(byte_col_to_utf16("const ñ = 1;", 8), 7);
        assert_eq!(byte_col_to_utf16("áéíóú", 10), 5);
    }

    #[test]
    fn mid_character_columns_clamp_to_the_character_start() {
        assert_eq!(byte_col_to_utf16("const ñ = 1;", 7), 6);
        assert_eq!(byte_col_to_utf16("😀a", 2), 0);
    }

    #[test]
    fn emoji_outside_the_bmp_count_two_code_units() {
        // "😀" is four UTF-8 bytes and two UTF-16 code units.
        assert_eq!(byte_col_to_utf16("a😀b", 0), 0);
        assert_eq!(byte_col_to_utf16("a😀b", 1), 1);
        assert_eq!(byte_col_to_utf16("a😀b", 5), 3);
        assert_eq!(byte_col_to_utf16("a😀b", 6), 4);
    }

    #[test]
    fn tabs_count_as_one_code_unit() {
        assert_eq!(byte_col_to_utf16("a\tb", 0), 0);
        assert_eq!(byte_col_to_utf16("a\tb", 1), 1);
        assert_eq!(byte_col_to_utf16("a\tb", 2), 2);
    }

    #[test]
    fn empty_line_has_column_zero_only() {
        assert_eq!(byte_col_to_utf16("", 0), 0);
        assert_eq!(byte_col_to_utf16("", 5), 0);
    }

    #[test]
    fn out_of_range_column_clamps_to_line_length() {
        assert_eq!(byte_col_to_utf16("abc", 10), 3);
        assert_eq!(byte_col_to_utf16("a😀", 99), 3);
    }
}
