//! Снаряжение: четыре слота, три редкости, усиление и Мастерская.
//!
//! Предмет даёт основную характеристику своего слота и доп. свойства (их число зависит от
//! редкости). Усиление +0…+15 без провала: растёт основная характеристика, каждые +3 —
//! одно доп. свойство. Создание и разбор — в Мастерской (её уровень открывает редкости).
//! Числа — `data/gear.json`.

use std::collections::HashMap;

use battle_core::{Rng, Stats};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::Sfx;
use crate::menu::{MUTED, MenuRebuild, PARCHMENT, Profile, Toast, pressable, text};
use crate::save::now;
use crate::style::{Look, Skin};
use crate::view::Fonts;

#[derive(Resource, Deserialize)]
pub struct GearData {
    pub slots: Vec<SlotDef>,
    pub rarities: Vec<RarityDef>,
    /// Значение одного доп. свойства по характеристикам.
    pub extras: HashMap<String, u32>,
    pub craft: CraftCost,
    pub enhance: EnhanceRules,
    /// Сколько деталей даёт разбор по редкости.
    pub dismantle_details: Vec<u32>,
}

#[derive(Deserialize)]
pub struct SlotDef {
    pub name: String,
    pub main: String,
    pub base: u32,
    pub per_level: u32,
}

#[derive(Deserialize)]
pub struct RarityDef {
    pub name: String,
    pub extras: usize,
    pub power: f32,
    pub weight: u32,
}

#[derive(Deserialize, Clone, Copy)]
pub struct CraftCost {
    pub details: u32,
    pub ether: u32,
    pub silver: u32,
}

#[derive(Deserialize)]
pub struct EnhanceRules {
    pub max: u32,
    pub silver_per_level: u32,
    pub details_per_level: u32,
    pub extra_every: u32,
}

const STATS: [&str; 4] = ["atk", "hp", "def", "spd"];

impl GearData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/gear.json"))
            .expect("data/gear.json корректен")
    }

    /// Создать предмет слота; редкость случайна, но не выше `max_rarity` (индекс).
    pub fn craft(&self, slot: usize, max_rarity: usize, rng: &mut Rng, id: u32) -> Item {
        let allowed = &self.rarities[..=max_rarity.min(self.rarities.len() - 1)];
        let total: u32 = allowed.iter().map(|r| r.weight).sum();
        let mut roll = (rng.next_f32() * total as f32) as u32;
        let mut rarity = 0;
        for (i, r) in allowed.iter().enumerate() {
            if roll < r.weight {
                rarity = i;
                break;
            }
            roll -= r.weight;
        }
        let main = &self.slots[slot].main;
        let pool: Vec<&str> = STATS.iter().copied().filter(|s| s != main).collect();
        let extras = (0..self.rarities[rarity].extras)
            .map(|_| {
                (
                    pool[(rng.next_f32() * pool.len() as f32) as usize % pool.len()].to_string(),
                    1,
                )
            })
            .collect();
        Item {
            id,
            slot,
            rarity,
            level: 0,
            extras,
        }
    }

    /// Что даёт предмет: основная характеристика и доп. свойства.
    pub fn stats(&self, item: &Item) -> Stats {
        let slot = &self.slots[item.slot];
        let power = self.rarities[item.rarity].power;
        let mut out = Stats {
            max_hp: 0,
            atk: 0,
            def: 0,
            spd: 0,
        };
        let mut add = |stat: &str, v: u32| match stat {
            "atk" => out.atk += v,
            "hp" => out.max_hp += v,
            "def" => out.def += v,
            _ => out.spd += v,
        };
        add(
            &slot.main,
            ((slot.base + slot.per_level * item.level) as f32 * power).round() as u32,
        );
        for (stat, rank) in &item.extras {
            let v = self.extras.get(stat).copied().unwrap_or(0) * rank;
            add(stat, (v as f32 * power).round() as u32);
        }
        out
    }

    pub fn enhance_cost(&self, item: &Item) -> (u32, u32) {
        let next = item.level + 1;
        (
            self.enhance.silver_per_level * next,
            self.enhance.details_per_level * next,
        )
    }

    /// Строка предмета: «Редкая · атака +18 · защита +5 · ур. +3».
    pub fn describe(&self, item: &Item) -> String {
        let s = self.stats(item);
        let mut parts = vec![self.rarities[item.rarity].name.clone()];
        for (v, name) in [
            (s.atk, "атака"),
            (s.max_hp, "здоровье"),
            (s.def, "защита"),
            (s.spd, "скорость"),
        ] {
            if v > 0 {
                parts.push(format!("{name} +{v}"));
            }
        }
        if item.level > 0 {
            parts.push(format!("+{}", item.level));
        }
        parts.join(" · ")
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u32,
    pub slot: usize,
    pub rarity: usize,
    /// Усиление +0…+15.
    pub level: u32,
    /// Доп. свойства: характеристика и её ранг (растёт при усилении).
    pub extras: Vec<(String, u32)>,
}

/// Склад снаряжения и кто что носит (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Gear {
    pub next_id: u32,
    pub items: Vec<Item>,
    /// Герой → id предметов по слотам.
    pub worn: HashMap<String, [Option<u32>; 4]>,
}

impl Gear {
    pub fn item(&self, id: u32) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn wearer(&self, id: u32) -> Option<&str> {
        self.worn
            .iter()
            .find(|(_, slots)| slots.contains(&Some(id)))
            .map(|(h, _)| h.as_str())
    }

    pub fn worn_by(&self, hero: &str) -> [Option<u32>; 4] {
        self.worn.get(hero).copied().unwrap_or([None; 4])
    }

    pub fn add(&mut self, mut item: Item) -> u32 {
        self.next_id += 1;
        item.id = self.next_id;
        self.items.push(item);
        self.next_id
    }

    /// Надеть на героя лучшие свободные предметы (по сумме характеристик).
    pub fn equip_best(&mut self, hero: &str, data: &GearData) {
        let mut slots = self.worn_by(hero);
        for (slot, current) in slots.iter_mut().enumerate() {
            let score = |id: u32| {
                self.item(id).map_or(0, |i| {
                    let s = data.stats(i);
                    s.atk * 10 + s.max_hp + s.def * 10 + s.spd * 15
                })
            };
            let best = self
                .items
                .iter()
                .filter(|i| i.slot == slot)
                .filter(|i| self.wearer(i.id).is_none_or(|w| w == hero))
                .max_by_key(|i| score(i.id))
                .map(|i| i.id);
            if let Some(best) = best
                && current.is_none_or(|c| score(best) > score(c))
            {
                *current = Some(best);
            }
        }
        self.worn.insert(hero.to_string(), slots);
    }

    pub fn unequip_all(&mut self, hero: &str) {
        self.worn.remove(hero);
    }

    /// Сколько прибавляет всё надетое на героя.
    pub fn bonus(&self, hero: &str, data: &GearData) -> Stats {
        let mut out = Stats {
            max_hp: 0,
            atk: 0,
            def: 0,
            spd: 0,
        };
        for id in self.worn_by(hero).into_iter().flatten() {
            if let Some(item) = self.item(id) {
                let s = data.stats(item);
                out.max_hp += s.max_hp;
                out.atk += s.atk;
                out.def += s.def;
                out.spd += s.spd;
            }
        }
        out
    }

    /// Усилить на +1; каждые `extra_every` уровней растёт одно доп. свойство.
    pub fn enhance(&mut self, id: u32, data: &GearData) -> bool {
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        if item.level >= data.enhance.max {
            return false;
        }
        item.level += 1;
        if item.level.is_multiple_of(data.enhance.extra_every) && !item.extras.is_empty() {
            let n = item.extras.len();
            let k = ((item.level / data.enhance.extra_every) as usize - 1) % n;
            item.extras[k].1 += 1;
        }
        true
    }

    /// Разобрать все не надетые предметы. Возвращает детали (усиление возвращается целиком).
    pub fn dismantle_spare(&mut self, data: &GearData) -> (u32, usize) {
        let worn: Vec<u32> = self
            .worn
            .values()
            .flat_map(|s| s.iter().flatten().copied())
            .collect();
        let mut details = 0;
        let mut count = 0;
        self.items.retain(|i| {
            if worn.contains(&i.id) {
                return true;
            }
            details += data.dismantle_details.get(i.rarity).copied().unwrap_or(0);
            details += data.enhance.details_per_level * i.level * (i.level + 1) / 2;
            count += 1;
            false
        });
        (details, count)
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(GearData::bundled())
        .init_resource::<Gear>();
}

/// Кнопки Мастерской в панели отсека: создать предмет слота и разобрать лишнее.
pub fn workshop_section(
    p: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &GearData,
    gear: &Gear,
    max_rarity: usize,
    profile: &Profile,
) {
    let c = data.craft;
    let can =
        profile.details >= c.details && profile.ether >= c.ether && profile.silver >= c.silver;
    p.spawn(text(
        fonts,
        format!(
            "Создать снаряжение: {} деталей, {} эфира, {} серебра · у вас деталей {} · на складе {}",
            c.details,
            c.ether,
            c.silver,
            profile.details,
            gear.items.len()
        ),
        false,
        12.0,
        MUTED,
    ));
    p.spawn(Node {
        column_gap: px(6),
        ..default()
    })
    .with_children(|row| {
        for (i, slot) in data.slots.iter().enumerate() {
            gear_button(row, fonts, &slot.name, can, GearAct::Craft(i, max_rarity));
        }
        gear_button(row, fonts, "Разобрать лишнее", true, GearAct::Dismantle);
    });
}

#[derive(Clone)]
pub enum GearAct {
    Craft(usize, usize),
    Dismantle,
}

fn gear_button(
    row: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    active: bool,
    act: GearAct,
) {
    let mut button = row.spawn((
        Node {
            flex_grow: 1.0,
            height: px(40),
            padding: UiRect::horizontal(px(4)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        Skin(if active { Look::Go } else { Look::Off }),
    ));
    button.with_child((
        text(fonts, label, true, 12.0, PARCHMENT),
        TextLayout::justify(Justify::Center),
    ));
    pressable(&mut button);
    button.observe(move |_: On<PointerClick>, mut commands: Commands| {
        commands.run_system_cached_with(gear_act, act.clone());
    });
}

fn gear_act(
    In(act): In<GearAct>,
    data: Res<GearData>,
    mut gear: ResMut<Gear>,
    mut profile: ResMut<Profile>,
    mut toast: ResMut<Toast>,
    mut rebuild: ResMut<MenuRebuild>,
    mut sfx: MessageWriter<Sfx>,
) {
    match act {
        GearAct::Craft(slot, max) => {
            let c = data.craft;
            if profile.details < c.details || profile.ether < c.ether || profile.silver < c.silver {
                toast.show("Не хватает: детали дают Бездна, экспедиции и разбор снаряжения");
                return;
            }
            profile.details -= c.details;
            profile.ether -= c.ether;
            profile.silver -= c.silver;
            let mut rng = Rng::new(now() ^ ((gear.next_id as u64) << 20));
            let item = data.craft(slot, max, &mut rng, 0);
            let epic = item.rarity >= 2;
            toast.show(&format!(
                "Создано — {}: {}",
                data.slots[slot].name,
                data.describe(&item)
            ));
            gear.add(item);
            sfx.write(Sfx(if epic { "ssr" } else { "done" }));
        }
        GearAct::Dismantle => {
            let (details, count) = gear.dismantle_spare(&data);
            if count == 0 {
                toast.show("Разбирать нечего: всё снаряжение надето");
                return;
            }
            profile.details += details;
            toast.show(&format!(
                "Разобрано предметов: {count} · +{details} деталей"
            ));
            sfx.write(Sfx("hammer"));
        }
    }
    rebuild.0 = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crafted_item_respects_rarity_cap_and_slot() {
        let data = GearData::bundled();
        let mut rng = Rng::new(1);
        for _ in 0..50 {
            let item = data.craft(0, 0, &mut rng, 0);
            assert_eq!(item.rarity, 0);
            assert!(data.stats(&item).atk > 0);
        }
    }

    #[test]
    fn enhance_raises_main_stat_and_caps() {
        let data = GearData::bundled();
        let mut gear = Gear::default();
        let id = gear.add(data.craft(1, 0, &mut Rng::new(2), 0));
        let before = data.stats(gear.item(id).unwrap()).max_hp;
        for _ in 0..data.enhance.max {
            assert!(gear.enhance(id, &data));
        }
        assert!(!gear.enhance(id, &data));
        assert!(data.stats(gear.item(id).unwrap()).max_hp > before);
    }

    #[test]
    fn equip_best_and_dismantle_spare() {
        let data = GearData::bundled();
        let mut gear = Gear::default();
        let mut rng = Rng::new(3);
        gear.add(data.craft(0, 2, &mut rng, 0));
        gear.add(data.craft(0, 2, &mut rng, 0));
        gear.equip_best("ren", &data);
        assert!(gear.bonus("ren", &data).atk > 0);
        let (details, count) = gear.dismantle_spare(&data);
        assert_eq!(count, 1);
        assert!(details > 0);
        assert_eq!(gear.items.len(), 1);
    }
}
