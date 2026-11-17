//! Данные игры: герои, враги и этапы из папки `data/`.
//!
//! Баланс живёт в JSON, а не в коде: его можно менять без пересборки логики,
//! а позже — загружать с сервера.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use battle_core::{Ability, Element, Role, Stats, UnitDef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rarity {
    #[serde(rename = "R")]
    R,
    #[serde(rename = "SR")]
    Sr,
    #[serde(rename = "SSR")]
    Ssr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeroDef {
    pub id: String,
    pub name: String,
    /// Короткое прозвище для карточки героя.
    pub title: String,
    pub rarity: Rarity,
    pub role: Role,
    pub element: Element,
    pub base_stats: Stats,
    /// Прибавка к характеристикам за каждый уровень после первого.
    pub growth: Stats,
    pub basic: Ability,
    pub skill: Ability,
    pub ultimate: Ability,
}

impl HeroDef {
    pub fn to_unit(&self, level: u32) -> UnitDef {
        UnitDef {
            id: self.id.clone(),
            name: self.name.clone(),
            element: self.element,
            role: Some(self.role),
            stats: scale(self.base_stats, self.growth, level),
            basic: self.basic.clone(),
            skill: Some(self.skill.clone()),
            ultimate: Some(self.ultimate.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyDef {
    pub id: String,
    pub name: String,
    pub element: Element,
    pub base_stats: Stats,
    pub growth: Stats,
    pub basic: Ability,
    #[serde(default)]
    pub skill: Option<Ability>,
}

impl EnemyDef {
    pub fn to_unit(&self, level: u32) -> UnitDef {
        UnitDef {
            id: self.id.clone(),
            name: self.name.clone(),
            element: self.element,
            role: None,
            stats: scale(self.base_stats, self.growth, level),
            basic: self.basic.clone(),
            skill: self.skill.clone(),
            ultimate: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageEnemy {
    pub enemy: String,
    pub level: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDef {
    pub id: String,
    pub name: String,
    pub enemies: Vec<StageEnemy>,
}

fn scale(base: Stats, growth: Stats, level: u32) -> Stats {
    let n = level.saturating_sub(1);
    Stats {
        max_hp: base.max_hp + growth.max_hp * n,
        atk: base.atk + growth.atk * n,
        def: base.def + growth.def * n,
        spd: base.spd + growth.spd * n,
    }
}

#[derive(Debug)]
pub enum DataError {
    Io {
        file: String,
        source: std::io::Error,
    },
    Json {
        file: String,
        source: serde_json::Error,
    },
    DuplicateId(String),
    UnknownEnemy {
        stage: String,
        enemy: String,
    },
    UnknownStage(String),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Io { file, source } => write!(f, "не удалось прочитать {file}: {source}"),
            DataError::Json { file, source } => write!(f, "ошибка в {file}: {source}"),
            DataError::DuplicateId(id) => write!(f, "id «{id}» встречается дважды"),
            DataError::UnknownEnemy { stage, enemy } => {
                write!(
                    f,
                    "этап «{stage}» ссылается на неизвестного врага «{enemy}»"
                )
            }
            DataError::UnknownStage(id) => write!(f, "нет этапа «{id}»"),
        }
    }
}

impl std::error::Error for DataError {}

#[derive(Clone, Debug, PartialEq)]
pub struct GameData {
    pub heroes: Vec<HeroDef>,
    pub enemies: Vec<EnemyDef>,
    pub stages: Vec<StageDef>,
}

const HEROES_FILE: &str = "heroes.json";
const ENEMIES_FILE: &str = "enemies.json";
const STAGES_FILE: &str = "stages.json";

impl GameData {
    /// Данные, вшитые в бинарник при сборке. Удобно для телефонов: не нужен доступ к файлам.
    pub fn bundled() -> Result<Self, DataError> {
        Self::from_json(
            include_str!("../../../data/heroes.json"),
            include_str!("../../../data/enemies.json"),
            include_str!("../../../data/stages.json"),
        )
    }

    /// Загрузка из папки (для быстрой правки баланса без пересборки).
    pub fn load_dir(dir: &Path) -> Result<Self, DataError> {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).map_err(|source| DataError::Io {
                file: name.into(),
                source,
            })
        };
        Self::from_json(
            &read(HEROES_FILE)?,
            &read(ENEMIES_FILE)?,
            &read(STAGES_FILE)?,
        )
    }

    pub fn from_json(heroes: &str, enemies: &str, stages: &str) -> Result<Self, DataError> {
        fn parse<T: for<'de> Deserialize<'de>>(file: &str, text: &str) -> Result<T, DataError> {
            serde_json::from_str(text).map_err(|source| DataError::Json {
                file: file.into(),
                source,
            })
        }
        let data = GameData {
            heroes: parse(HEROES_FILE, heroes)?,
            enemies: parse(ENEMIES_FILE, enemies)?,
            stages: parse(STAGES_FILE, stages)?,
        };
        data.validate()?;
        Ok(data)
    }

    fn validate(&self) -> Result<(), DataError> {
        let mut seen = HashSet::new();
        let ids = self
            .heroes
            .iter()
            .map(|h| &h.id)
            .chain(self.enemies.iter().map(|e| &e.id))
            .chain(self.stages.iter().map(|s| &s.id));
        for id in ids {
            if !seen.insert(id) {
                return Err(DataError::DuplicateId(id.clone()));
            }
        }
        for stage in &self.stages {
            for e in &stage.enemies {
                if self.enemy(&e.enemy).is_none() {
                    return Err(DataError::UnknownEnemy {
                        stage: stage.id.clone(),
                        enemy: e.enemy.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn hero(&self, id: &str) -> Option<&HeroDef> {
        self.heroes.iter().find(|h| h.id == id)
    }

    pub fn enemy(&self, id: &str) -> Option<&EnemyDef> {
        self.enemies.iter().find(|e| e.id == id)
    }

    pub fn stage(&self, id: &str) -> Option<&StageDef> {
        self.stages.iter().find(|s| s.id == id)
    }

    /// Враги этапа, готовые к бою.
    pub fn stage_enemies(&self, stage_id: &str) -> Result<Vec<UnitDef>, DataError> {
        let stage = self
            .stage(stage_id)
            .ok_or_else(|| DataError::UnknownStage(stage_id.into()))?;
        stage
            .enemies
            .iter()
            .map(|e| {
                self.enemy(&e.enemy)
                    .map(|def| def.to_unit(e.level))
                    .ok_or_else(|| DataError::UnknownEnemy {
                        stage: stage.id.clone(),
                        enemy: e.enemy.clone(),
                    })
            })
            .collect()
    }
}
