//! Описание бойцов: стихии, роли, характеристики и умения.

use serde::{Deserialize, Serialize};

/// Стихия героя или врага. Атака по слабости даёт бонус к урону.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Wind,
    Lightning,
    Powder,
    Steel,
    Mist,
}

impl Element {
    /// Круг стихий: каждая сильна против следующей.
    /// Ветер → Туман → Молния → Сталь → Порох → Ветер.
    pub fn beats(self) -> Element {
        match self {
            Element::Wind => Element::Mist,
            Element::Mist => Element::Lightning,
            Element::Lightning => Element::Steel,
            Element::Steel => Element::Powder,
            Element::Powder => Element::Wind,
        }
    }

    pub fn is_strong_against(self, other: Element) -> bool {
        self.beats() == other
    }
}

/// Роль героя в экипаже. В бою влияет только на решения автобоя.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Канонир: урон по площади.
    Gunner,
    /// Дуэлянт: урон по одной цели.
    Duelist,
    /// Абордажник: танк.
    Boarder,
    /// Навигатор: ускоряет своих, замедляет врагов.
    Navigator,
    /// Корабельный врач: лечение.
    Medic,
    /// Шаман ветра: контроль.
    WindShaman,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Stats {
    pub max_hp: u32,
    pub atk: u32,
    pub def: u32,
    pub spd: u32,
}

/// На кого действует эффект. «Враги» и «союзники» считаются относительно того, кто действует.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    SingleEnemy,
    AllEnemies,
    SingleAlly,
    AllAllies,
    Myself,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Урон: множитель от атаки действующего бойца.
    Damage { multiplier: f32 },
    /// Лечение: доля от максимального здоровья цели (0.3 = 30%).
    Heal { percent: f32 },
    /// Сдвинуть цель вперёд по шкале ходов: доля от её полного хода (0.25 = 25%).
    Advance { fraction: f32 },
    /// Отодвинуть цель назад по шкале ходов.
    Delay { fraction: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetedEffect {
    pub target: Target,
    #[serde(flatten)]
    pub effect: Effect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub effects: Vec<TargetedEffect>,
    /// Только для умения: раз во сколько своих ходов оно срабатывает (3 — на каждом третьем).
    /// Для обычной атаки и ульты не используется.
    #[serde(default)]
    pub cooldown: u32,
}

impl Ability {
    pub fn heals(&self) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e.effect, Effect::Heal { .. }))
    }
}

/// Готовый к бою боец: характеристики уже посчитаны с учётом уровня и снаряжения.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    pub element: Element,
    #[serde(default)]
    pub role: Option<Role>,
    pub stats: Stats,
    pub basic: Ability,
    #[serde(default)]
    pub skill: Option<Ability>,
    #[serde(default)]
    pub ultimate: Option<Ability>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnitId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Ally,
    Enemy,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Ally => Side::Enemy,
            Side::Enemy => Side::Ally,
        }
    }
}

/// Боец во время боя.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub side: Side,
    pub def: UnitDef,
    pub hp: u32,
    /// Энергия для ульты, от 0 до [`crate::ENERGY_MAX`].
    pub energy: u32,
    /// Сколько «расстояния» осталось до хода. Ходит боец с наименьшим значением.
    pub action_value: f32,
    pub turns_taken: u32,
    /// Сколько своих ходов осталось до умения; 0 — умение готово.
    pub skill_cooldown: u32,
}

impl Unit {
    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    pub fn hp_ratio(&self) -> f32 {
        self.hp as f32 / self.def.stats.max_hp.max(1) as f32
    }

    /// Полная длина хода: чем выше скорость, тем короче.
    pub fn base_action_value(&self) -> f32 {
        crate::ACTION_GAUGE / self.def.stats.spd.max(1) as f32
    }

    pub fn skill_ready(&self) -> bool {
        self.is_alive() && self.def.skill.is_some() && self.skill_cooldown == 0
    }

    /// Перезарядка умения в начале боя и после его применения.
    pub fn skill_cooldown_start(&self) -> u32 {
        self.def
            .skill
            .as_ref()
            .map_or(0, |s| s.cooldown.saturating_sub(1))
    }

    pub fn ultimate_ready(&self) -> bool {
        self.is_alive() && self.def.ultimate.is_some() && self.energy >= crate::ENERGY_MAX
    }
}
