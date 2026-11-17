//! Тексты для игрока. Позже здесь будет локализация.

use battle_core::Element;

pub fn element_name(element: Element) -> &'static str {
    match element {
        Element::Wind => "Ветер",
        Element::Lightning => "Молния",
        Element::Powder => "Порох",
        Element::Steel => "Сталь",
        Element::Mist => "Туман",
    }
}
