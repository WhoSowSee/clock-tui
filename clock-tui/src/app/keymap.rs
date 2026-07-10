use crossterm::event::KeyCode;

/// Map a printed char to its QWERTY position, so hotkeys work in any
/// keyboard layout (EN/RU/etc). Returns `None` if the char has no
/// direct neighbour on a US-QWERTY keyboard.
pub fn qwerty_position(c: char) -> Option<char> {
    const EN_TO_RU: &[(char, char)] = &[
        ('q', 'й'), ('w', 'ц'), ('e', 'у'), ('r', 'к'), ('t', 'е'),
        ('y', 'н'), ('u', 'г'), ('i', 'ш'), ('o', 'щ'), ('p', 'з'),
        ('[', 'х'), (']', 'ъ'),
        ('a', 'ф'), ('s', 'ы'), ('d', 'в'), ('f', 'а'), ('g', 'п'),
        ('h', 'р'), ('j', 'о'), ('k', 'л'), ('l', 'д'),
        (';', 'ж'), ('\'', 'э'),
        ('z', 'я'), ('x', 'ч'), ('c', 'с'), ('v', 'м'),
        ('b', 'и'), ('n', 'т'), ('m', 'ь'),
        (',', 'б'), ('.', 'ю'),
    ];
    let lc = c.to_ascii_lowercase();
    if lc.is_ascii_alphanumeric() {
        return Some(lc);
    }
    for &(en, ru) in EN_TO_RU {
        if en == lc {
            return Some(ru);
        }
    }
    for &(en, ru) in EN_TO_RU {
        if ru == lc {
            return Some(en);
        }
    }
    None
}

pub fn map_to_ascii(c: char) -> Option<char> {
    match qwerty_position(c) {
        Some(ch) if ch.is_ascii() => Some(ch),
        _ => None,
    }
}

/// Translate a Char key event through QWERTY position so that
/// `d` fires whether the layout is EN (`d`) or RU (`в`).
pub fn layout_aware(key: KeyCode) -> KeyCode {
    if let KeyCode::Char(c) = key {
        if let Some(m) = map_to_ascii(c) {
            return KeyCode::Char(m);
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn ascii_passthrough() {
        assert_eq!(map_to_ascii('d'), Some('d'));
        assert_eq!(map_to_ascii('D'), Some('d'));
        assert_eq!(map_to_ascii('q'), Some('q'));
    }

    #[test]
    fn ru_layout_maps_to_en() {
        assert_eq!(map_to_ascii('в'), Some('d')); // d position
        assert_eq!(map_to_ascii('ы'), Some('s')); // s position
        assert_eq!(map_to_ascii('ь'), Some('m')); // m position
        assert_eq!(map_to_ascii('й'), Some('q')); // q position
        assert_eq!(map_to_ascii('ц'), Some('w')); // w position
        assert_eq!(map_to_ascii('е'), Some('t')); // t position
    }

    #[test]
    fn layout_aware_wraps_char() {
        assert_eq!(layout_aware(KeyCode::Char('d')), KeyCode::Char('d'));
        assert_eq!(layout_aware(KeyCode::Char('в')), KeyCode::Char('d'));
        assert_eq!(layout_aware(KeyCode::Char('ы')), KeyCode::Char('s'));
        assert_eq!(layout_aware(KeyCode::Char(' ')), KeyCode::Char(' '));
        assert_eq!(layout_aware(KeyCode::Esc), KeyCode::Esc);
    }
}
