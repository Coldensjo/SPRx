// Tibia .dat metadata reader, ported from SpriteForge's dat_reader.rs but
// table-driven and with automatic client-version detection: every behavior-
// distinct parser configuration is tried and the one that consumes the file
// exactly to EOF wins.

use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::sync::{Arc, Mutex};

use crate::spr::{decompress_to_rgba, SprManager, SPRITE_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Item,
    Outfit,
    Effect,
    Missile,
}

impl Category {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "item" => Some(Self::Item),
            "outfit" => Some(Self::Outfit),
            "effect" => Some(Self::Effect),
            "missile" => Some(Self::Missile),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingProp {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thing {
    pub id: u32,
    pub width: u8,
    pub height: u8,
    pub exact_size: u8,
    pub layers: u8,
    pub pattern_x: u8,
    pub pattern_y: u8,
    pub pattern_z: u8,
    pub frames: u8,
    pub sprite_index: Vec<u32>,
    pub props: Vec<ThingProp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub is_outfit: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatInfo {
    pub signature: u32,
    pub version: u32,
    pub item_first_id: u32,
    pub item_last_id: u32,
    pub outfit_count: u32,
    pub effect_count: u32,
    pub missile_count: u32,
}

pub struct DatFile {
    pub info: DatInfo,
    pub items: Vec<Thing>,
    pub outfits: Vec<Thing>,
    pub effects: Vec<Thing>,
    pub missiles: Vec<Thing>,
}

impl DatFile {
    pub fn things(&self, cat: Category) -> &Vec<Thing> {
        match cat {
            Category::Item => &self.items,
            Category::Outfit => &self.outfits,
            Category::Effect => &self.effects,
            Category::Missile => &self.missiles,
        }
    }

    pub fn thing(&self, cat: Category, id: u32) -> Option<&Thing> {
        let list = self.things(cat);
        let first = if cat == Category::Item { 100 } else { 1 };
        if id < first {
            return None;
        }
        list.get((id - first) as usize)
    }
}

// ---------- Flag tables (one per client-version era, from SpriteForge) ----------

#[derive(Clone, Copy)]
enum Extra {
    None,
    U16,
    U16x2,
    OffsetLegacy, // no payload, implicit 8,8
    Market,
    Bones,
    Ignored, // accepted, no payload, not recorded
}

type FlagSpec = (u8, &'static str, Extra);

const FLAGS_V1: &[FlagSpec] = &[
    (0x00, "ground", Extra::U16),
    (0x01, "onBottom", Extra::None),
    (0x02, "onTop", Extra::None),
    (0x03, "container", Extra::None),
    (0x04, "stackable", Extra::None),
    (0x05, "multiUse", Extra::None),
    (0x06, "forceUse", Extra::None),
    (0x07, "writable", Extra::U16),
    (0x08, "writableOnce", Extra::U16),
    (0x09, "fluidContainer", Extra::None),
    (0x0A, "fluid", Extra::None),
    (0x0B, "unpassable", Extra::None),
    (0x0C, "unmoveable", Extra::None),
    (0x0D, "blockMissile", Extra::None),
    (0x0E, "blockPathfind", Extra::None),
    (0x0F, "pickupable", Extra::None),
    (0x10, "light", Extra::U16x2),
    (0x11, "floorChange", Extra::None),
    (0x12, "fullGround", Extra::None),
    (0x13, "elevation", Extra::U16),
    (0x14, "offset", Extra::OffsetLegacy),
    (0x15, "", Extra::Ignored),
    (0x16, "miniMap", Extra::U16),
    (0x17, "rotatable", Extra::None),
    (0x18, "lyingObject", Extra::None),
    (0x19, "animateAlways", Extra::None),
    (0x1A, "lensHelp", Extra::U16),
    (0x24, "wrappable", Extra::None),
    (0x25, "unwrappable", Extra::None),
    (0x26, "topEffect", Extra::None),
];

const FLAGS_V2: &[FlagSpec] = &[
    (0x00, "ground", Extra::U16),
    (0x01, "onBottom", Extra::None),
    (0x02, "onTop", Extra::None),
    (0x03, "container", Extra::None),
    (0x04, "stackable", Extra::None),
    (0x05, "multiUse", Extra::None),
    (0x06, "forceUse", Extra::None),
    (0x07, "writable", Extra::U16),
    (0x08, "writableOnce", Extra::U16),
    (0x09, "fluidContainer", Extra::None),
    (0x0A, "fluid", Extra::None),
    (0x0B, "unpassable", Extra::None),
    (0x0C, "unmoveable", Extra::None),
    (0x0D, "blockMissile", Extra::None),
    (0x0E, "blockPathfind", Extra::None),
    (0x0F, "pickupable", Extra::None),
    (0x10, "light", Extra::U16x2),
    (0x11, "floorChange", Extra::None),
    (0x12, "fullGround", Extra::None),
    (0x13, "elevation", Extra::U16),
    (0x14, "offset", Extra::OffsetLegacy),
    (0x15, "", Extra::Ignored),
    (0x16, "miniMap", Extra::U16),
    (0x17, "rotatable", Extra::None),
    (0x18, "lyingObject", Extra::None),
    (0x19, "hangable", Extra::None),
    (0x1A, "vertical", Extra::None),
    (0x1B, "horizontal", Extra::None),
    (0x1C, "animateAlways", Extra::None),
    (0x1D, "lensHelp", Extra::U16),
    (0x24, "wrappable", Extra::None),
    (0x25, "unwrappable", Extra::None),
    (0x26, "topEffect", Extra::None),
];

const FLAGS_V3: &[FlagSpec] = &[
    (0x00, "ground", Extra::U16),
    (0x01, "groundBorder", Extra::None),
    (0x02, "onBottom", Extra::None),
    (0x03, "onTop", Extra::None),
    (0x04, "container", Extra::None),
    (0x05, "stackable", Extra::None),
    (0x06, "forceUse", Extra::None),
    (0x07, "multiUse", Extra::None),
    (0x08, "writable", Extra::U16),
    (0x09, "writableOnce", Extra::U16),
    (0x0A, "fluidContainer", Extra::None),
    (0x0B, "fluid", Extra::None),
    (0x0C, "unpassable", Extra::None),
    (0x0D, "unmoveable", Extra::None),
    (0x0E, "blockMissile", Extra::None),
    (0x0F, "blockPathfind", Extra::None),
    (0x10, "pickupable", Extra::None),
    (0x11, "hangable", Extra::None),
    (0x12, "vertical", Extra::None),
    (0x13, "horizontal", Extra::None),
    (0x14, "rotatable", Extra::None),
    (0x15, "light", Extra::U16x2),
    (0x16, "", Extra::Ignored),
    (0x17, "floorChange", Extra::None),
    (0x18, "offset", Extra::U16x2),
    (0x19, "elevation", Extra::U16),
    (0x1A, "lyingObject", Extra::None),
    (0x1B, "animateAlways", Extra::None),
    (0x1C, "miniMap", Extra::U16),
    (0x1D, "lensHelp", Extra::U16),
    (0x1E, "fullGround", Extra::None),
];

const FLAGS_V4: &[FlagSpec] = &[
    (0x00, "ground", Extra::U16),
    (0x01, "groundBorder", Extra::None),
    (0x02, "onBottom", Extra::None),
    (0x03, "onTop", Extra::None),
    (0x04, "container", Extra::None),
    (0x05, "stackable", Extra::None),
    (0x06, "forceUse", Extra::None),
    (0x07, "multiUse", Extra::None),
    (0x08, "hasCharges", Extra::None),
    (0x09, "writable", Extra::U16),
    (0x0A, "writableOnce", Extra::U16),
    (0x0B, "fluidContainer", Extra::None),
    (0x0C, "fluid", Extra::None),
    (0x0D, "unpassable", Extra::None),
    (0x0E, "unmoveable", Extra::None),
    (0x0F, "blockMissile", Extra::None),
    (0x10, "blockPathfind", Extra::None),
    (0x11, "pickupable", Extra::None),
    (0x12, "hangable", Extra::None),
    (0x13, "vertical", Extra::None),
    (0x14, "horizontal", Extra::None),
    (0x15, "rotatable", Extra::None),
    (0x16, "light", Extra::U16x2),
    (0x17, "dontHide", Extra::None),
    (0x18, "floorChange", Extra::None),
    (0x19, "offset", Extra::U16x2),
    (0x1A, "elevation", Extra::U16),
    (0x1B, "lyingObject", Extra::None),
    (0x1C, "animateAlways", Extra::None),
    (0x1D, "miniMap", Extra::U16),
    (0x1E, "lensHelp", Extra::U16),
    (0x1F, "fullGround", Extra::None),
    (0x20, "ignoreLook", Extra::None),
    (0x24, "wrappable", Extra::None),
    (0x25, "unwrappable", Extra::None),
    (0x27, "bones", Extra::Bones),
];

const FLAGS_V5: &[FlagSpec] = &[
    (0x00, "ground", Extra::U16),
    (0x01, "groundBorder", Extra::None),
    (0x02, "onBottom", Extra::None),
    (0x03, "onTop", Extra::None),
    (0x04, "container", Extra::None),
    (0x05, "stackable", Extra::None),
    (0x06, "forceUse", Extra::None),
    (0x07, "multiUse", Extra::None),
    (0x08, "writable", Extra::U16),
    (0x09, "writableOnce", Extra::U16),
    (0x0A, "fluidContainer", Extra::None),
    (0x0B, "fluid", Extra::None),
    (0x0C, "unpassable", Extra::None),
    (0x0D, "unmoveable", Extra::None),
    (0x0E, "blockMissile", Extra::None),
    (0x0F, "blockPathfind", Extra::None),
    (0x10, "pickupable", Extra::None),
    (0x11, "hangable", Extra::None),
    (0x12, "vertical", Extra::None),
    (0x13, "horizontal", Extra::None),
    (0x14, "rotatable", Extra::None),
    (0x15, "light", Extra::U16x2),
    (0x16, "dontHide", Extra::None),
    (0x17, "translucent", Extra::None),
    (0x18, "offset", Extra::U16x2),
    (0x19, "elevation", Extra::U16),
    (0x1A, "lyingObject", Extra::None),
    (0x1B, "animateAlways", Extra::None),
    (0x1C, "miniMap", Extra::U16),
    (0x1D, "lensHelp", Extra::U16),
    (0x1E, "fullGround", Extra::None),
    (0x1F, "ignoreLook", Extra::None),
    (0x20, "cloth", Extra::U16),
    (0x21, "market", Extra::Market),
    (0x27, "bones", Extra::Bones),
];

const FLAGS_V6: &[FlagSpec] = &[
    (0x00, "ground", Extra::U16),
    (0x01, "groundBorder", Extra::None),
    (0x02, "onBottom", Extra::None),
    (0x03, "onTop", Extra::None),
    (0x04, "container", Extra::None),
    (0x05, "stackable", Extra::None),
    (0x06, "forceUse", Extra::None),
    (0x07, "multiUse", Extra::None),
    (0x08, "writable", Extra::U16),
    (0x09, "writableOnce", Extra::U16),
    (0x0A, "fluidContainer", Extra::None),
    (0x0B, "fluid", Extra::None),
    (0x0C, "unpassable", Extra::None),
    (0x0D, "unmoveable", Extra::None),
    (0x0E, "blockMissile", Extra::None),
    (0x0F, "blockPathfind", Extra::None),
    (0x10, "noMoveAnimation", Extra::None),
    (0x11, "pickupable", Extra::None),
    (0x12, "hangable", Extra::None),
    (0x13, "vertical", Extra::None),
    (0x14, "horizontal", Extra::None),
    (0x15, "rotatable", Extra::None),
    (0x16, "light", Extra::U16x2),
    (0x17, "dontHide", Extra::None),
    (0x18, "translucent", Extra::None),
    (0x19, "offset", Extra::U16x2),
    (0x1A, "elevation", Extra::U16),
    (0x1B, "lyingObject", Extra::None),
    (0x1C, "animateAlways", Extra::None),
    (0x1D, "miniMap", Extra::U16),
    (0x1E, "lensHelp", Extra::U16),
    (0x1F, "fullGround", Extra::None),
    (0x20, "ignoreLook", Extra::None),
    (0x21, "cloth", Extra::U16),
    (0x22, "market", Extra::Market),
    (0x23, "defaultAction", Extra::U16),
    (0x24, "wrappable", Extra::None),
    (0x25, "unwrappable", Extra::None),
    (0x26, "topEffect", Extra::None),
    (0x27, "bones", Extra::Bones),
    (0xFE, "usable", Extra::None),
];

fn flags_for_version(version: u32) -> &'static [FlagSpec] {
    if version <= 730 {
        FLAGS_V1
    } else if version <= 750 {
        FLAGS_V2
    } else if version <= 772 {
        FLAGS_V3
    } else if version <= 854 {
        FLAGS_V4
    } else if version <= 986 {
        FLAGS_V5
    } else {
        FLAGS_V6
    }
}

// ---------- Reader ----------

struct DatReader {
    reader: BufReader<File>,
    version: u32,
    extended: bool,
    frame_durations: bool,
    frame_groups: bool,
    file_len: u64,
}

impl DatReader {
    fn open(path: &str, version: u32) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Failed to open DAT file: {}", e))?;
        let file_len = file.metadata().map_err(|e| e.to_string())?.len();
        Ok(Self {
            reader: BufReader::new(file),
            version,
            extended: version >= 960,
            frame_durations: version >= 1050,
            frame_groups: version >= 1057,
            file_len,
        })
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        self.reader.read_exact(&mut b)?;
        Ok(b[0])
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        let mut b = [0u8; 2];
        self.reader.read_exact(&mut b)?;
        Ok(u16::from_le_bytes(b))
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        let mut b = [0u8; 4];
        self.reader.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn read_string(&mut self) -> io::Result<String> {
        let len = self.read_u16()?;
        let mut buf = vec![0u8; len as usize];
        self.reader.read_exact(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    fn read_properties(&mut self, thing: &mut Thing) -> io::Result<()> {
        let table = flags_for_version(self.version);
        loop {
            let flag = self.read_u8()?;
            if flag == 0xFF {
                break;
            }
            let spec = table.iter().find(|(f, _, _)| *f == flag).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unknown flag 0x{:02X} (dat version {})", flag, self.version),
                )
            })?;
            let (_, name, extra) = *spec;
            match extra {
                Extra::None => thing.props.push(ThingProp {
                    name: name.to_string(),
                    value: None,
                }),
                Extra::Ignored => {}
                Extra::U16 => {
                    let v = self.read_u16()?;
                    thing.props.push(ThingProp {
                        name: name.to_string(),
                        value: Some(v.to_string()),
                    });
                }
                Extra::U16x2 => {
                    let a = self.read_u16()?;
                    let b = self.read_u16()?;
                    thing.props.push(ThingProp {
                        name: name.to_string(),
                        value: Some(format!("{}, {}", a as i16, b as i16)),
                    });
                }
                Extra::OffsetLegacy => thing.props.push(ThingProp {
                    name: name.to_string(),
                    value: Some("8, 8".to_string()),
                }),
                Extra::Market => {
                    let category = self.read_u16()?;
                    let trade_as = self.read_u16()?;
                    let show_as = self.read_u16()?;
                    let market_name = self.read_string()?;
                    let profession = self.read_u16()?;
                    let level = self.read_u16()?;
                    thing.props.push(ThingProp {
                        name: "market".to_string(),
                        value: Some(format!(
                            "\"{}\" cat {} tradeAs {} showAs {} prof {} lvl {}",
                            market_name, category, trade_as, show_as, profession, level
                        )),
                    });
                    thing.name = Some(market_name);
                }
                Extra::Bones => {
                    let mut parts = Vec::with_capacity(4);
                    for _ in 0..4 {
                        let x = self.read_u16()? as i16;
                        let y = self.read_u16()? as i16;
                        parts.push(format!("({}, {})", x, y));
                    }
                    thing.props.push(ThingProp {
                        name: "bones".to_string(),
                        value: Some(parts.join(" ")),
                    });
                }
            }
        }
        Ok(())
    }

    fn read_texture_patterns(&mut self, thing: &mut Thing) -> io::Result<()> {
        let has_frame_groups = self.frame_groups && thing.is_outfit;
        let group_count = if has_frame_groups { self.read_u8()? } else { 1 };

        for group_idx in 0..group_count {
            if has_frame_groups {
                self.read_u8()?; // group type
            }

            let width = self.read_u8()?;
            let height = self.read_u8()?;
            let exact_size = if width > 1 || height > 1 { self.read_u8()? } else { 32 };
            let layers = self.read_u8()?;
            let pattern_x = self.read_u8()?;
            let pattern_y = self.read_u8()?;
            let pattern_z = if self.version <= 750 { 1 } else { self.read_u8()? };
            let frames = self.read_u8()?;

            if frames > 1 && self.frame_durations {
                self.read_u8()?; // animation mode
                self.read_u32()?; // loop count
                self.read_u8()?; // start frame
                for _ in 0..frames {
                    self.read_u32()?; // min duration
                    self.read_u32()?; // max duration
                }
            }

            let total = width as u32
                * height as u32
                * layers as u32
                * pattern_x as u32
                * pattern_y as u32
                * pattern_z as u32
                * frames as u32;
            if total == 0 || total > 4096 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Frame group has {} sprites (out of range)", total),
                ));
            }

            let mut sprite_index = Vec::with_capacity(total as usize);
            for _ in 0..total {
                let sid = if self.extended {
                    self.read_u32()?
                } else {
                    self.read_u16()? as u32
                };
                sprite_index.push(sid);
            }

            // Keep the first group (idle) as the thing's primary layout.
            if group_idx == 0 {
                thing.width = width;
                thing.height = height;
                thing.exact_size = exact_size;
                thing.layers = layers;
                thing.pattern_x = pattern_x;
                thing.pattern_y = pattern_y;
                thing.pattern_z = pattern_z;
                thing.frames = frames;
                thing.sprite_index = sprite_index;
            }
        }

        Ok(())
    }

    fn read_thing(&mut self, id: u32, is_outfit: bool) -> io::Result<Thing> {
        let mut thing = Thing {
            id,
            width: 1,
            height: 1,
            exact_size: 32,
            layers: 1,
            pattern_x: 1,
            pattern_y: 1,
            pattern_z: 1,
            frames: 1,
            sprite_index: Vec::new(),
            props: Vec::new(),
            name: None,
            is_outfit,
        };
        self.read_properties(&mut thing)?;
        self.read_texture_patterns(&mut thing)?;
        Ok(thing)
    }

    fn read_dat(&mut self) -> Result<DatFile, String> {
        let signature = self.read_u32().map_err(|e| format!("Failed to read signature: {}", e))?;
        let items_last = self.read_u16().map_err(|e| format!("Failed to read items count: {}", e))?;
        let outfits_count = self.read_u16().map_err(|e| format!("Failed to read outfits count: {}", e))?;
        let effects_count = self.read_u16().map_err(|e| format!("Failed to read effects count: {}", e))?;
        let missiles_count = self.read_u16().map_err(|e| format!("Failed to read missiles count: {}", e))?;

        if items_last < 100 {
            return Err("Invalid .dat header: item count below 100".to_string());
        }

        let mut items = Vec::with_capacity((items_last - 99) as usize);
        for id in 100..=items_last as u32 {
            items.push(
                self.read_thing(id, false)
                    .map_err(|e| format!("item {}: {}", id, e))?,
            );
        }
        let mut outfits = Vec::with_capacity(outfits_count as usize);
        for id in 1..=outfits_count as u32 {
            outfits.push(
                self.read_thing(id, true)
                    .map_err(|e| format!("outfit {}: {}", id, e))?,
            );
        }
        let mut effects = Vec::with_capacity(effects_count as usize);
        for id in 1..=effects_count as u32 {
            effects.push(
                self.read_thing(id, false)
                    .map_err(|e| format!("effect {}: {}", id, e))?,
            );
        }
        let mut missiles = Vec::with_capacity(missiles_count as usize);
        for id in 1..=missiles_count as u32 {
            missiles.push(
                self.read_thing(id, false)
                    .map_err(|e| format!("missile {}: {}", id, e))?,
            );
        }

        // A correct configuration consumes the file exactly.
        let pos = self.reader.stream_position().map_err(|e| e.to_string())?;
        if pos != self.file_len {
            return Err(format!(
                "Parsed OK but {} trailing bytes remain (wrong version guess)",
                self.file_len - pos
            ));
        }

        Ok(DatFile {
            info: DatInfo {
                signature,
                version: self.version,
                item_first_id: 100,
                item_last_id: items_last as u32,
                outfit_count: outfits_count as u32,
                effect_count: effects_count as u32,
                missile_count: missiles_count as u32,
            },
            items,
            outfits,
            effects,
            missiles,
        })
    }
}

/// One representative version per distinct parser behavior, newest first.
const CANDIDATE_VERSIONS: &[u32] = &[1098, 1050, 1010, 960, 900, 780, 760, 740, 710];

pub fn open_dat_auto(path: &str, force_version: Option<u32>) -> Result<DatFile, String> {
    if let Some(v) = force_version {
        let mut reader = DatReader::open(path, v)?;
        return reader.read_dat().map_err(|e| format!("DAT parse error (version {}): {}", v, e));
    }

    let mut last_err = String::new();
    for &v in CANDIDATE_VERSIONS {
        let mut reader = DatReader::open(path, v)?;
        match reader.read_dat() {
            Ok(dat) => return Ok(dat),
            Err(e) => last_err = format!("v{}: {}", v, e),
        }
    }
    Err(format!(
        "Could not auto-detect .dat version; no known layout parses cleanly. Last error: {}",
        last_err
    ))
}

// ---------- Composition ----------

const TILE: usize = SPRITE_SIZE; // 32
const TILE_BYTES: usize = TILE * TILE * 4;

fn sprite_slot(t: &Thing, frame: u32, pz: u32, py: u32, px: u32, layer: u32, ty: u32, tx: u32) -> usize {
    let w = t.width as u32;
    let h = t.height as u32;
    ((((((frame * t.pattern_z as u32 + pz) * t.pattern_y as u32 + py) * t.pattern_x as u32 + px)
        * t.layers as u32
        + layer)
        * h
        + ty)
        * w
        + tx) as usize
}

fn blend_tile(canvas: &mut [u8], canvas_w: usize, dst_x: usize, dst_y: usize, tile: &[u8]) {
    for y in 0..TILE {
        for x in 0..TILE {
            let s = (y * TILE + x) * 4;
            let sa = tile[s + 3] as u32;
            if sa == 0 {
                continue;
            }
            let d = ((dst_y + y) * canvas_w + dst_x + x) * 4;
            if sa == 255 {
                canvas[d..d + 4].copy_from_slice(&tile[s..s + 4]);
            } else {
                let da = canvas[d + 3] as u32;
                let out_a = sa + da * (255 - sa) / 255;
                for c in 0..3 {
                    let sc = tile[s + c] as u32;
                    let dc = canvas[d + c] as u32;
                    canvas[d + c] = if out_a == 0 {
                        0
                    } else {
                        ((sc * sa + dc * da * (255 - sa) / 255) / out_a) as u8
                    };
                }
                canvas[d + 3] = out_a as u8;
            }
        }
    }
}

pub struct ThingRender {
    pub width_px: u32,
    pub height_px: u32,
    pub rgba: Vec<u8>,
}

/// Which layers a cell renders: a specific one, or the blended default —
/// outfits blend only layer 0 (layer 1 is the colorization template).
fn cell_layers(t: &Thing, layer: Option<u32>) -> Vec<u32> {
    match layer {
        Some(l) => vec![l],
        None if t.is_outfit => vec![0],
        None => (0..t.layers as u32).collect(),
    }
}

/// Sprite ids one cell needs (skips empty slots).
fn cell_sprite_ids(t: &Thing, frame: u32, px: u32, py: u32, pz: u32, layer: Option<u32>, out: &mut Vec<u32>) {
    for &l in &cell_layers(t, layer) {
        for ty in 0..t.height as u32 {
            for tx in 0..t.width as u32 {
                if let Some(&sid) = t.sprite_index.get(sprite_slot(t, frame, pz, py, px, l, ty, tx)) {
                    if sid != 0 {
                        out.push(sid);
                    }
                }
            }
        }
    }
}

/// Composes one "cell" of a thing from already-decoded sprites. Tile (0,0) in
/// the sprite index is the bottom-right corner, matching the client's draw order.
pub fn compose_from_decoded(
    decoded: &HashMap<u32, Vec<u8>>,
    t: &Thing,
    frame: u32,
    px: u32,
    py: u32,
    pz: u32,
    layer: Option<u32>,
) -> ThingRender {
    let w = t.width as usize;
    let h = t.height as usize;
    let canvas_w = w * TILE;
    let canvas_h = h * TILE;
    let mut canvas = vec![0u8; canvas_w * canvas_h * 4];

    for &l in &cell_layers(t, layer) {
        for ty in 0..h as u32 {
            for tx in 0..w as u32 {
                let slot = sprite_slot(t, frame, pz, py, px, l, ty, tx);
                let Some(&sid) = t.sprite_index.get(slot) else { continue };
                if sid == 0 {
                    continue;
                }
                let Some(tile) = decoded.get(&sid) else { continue };
                debug_assert_eq!(tile.len(), TILE_BYTES);
                let dst_x = (w - 1 - tx as usize) * TILE;
                let dst_y = (h - 1 - ty as usize) * TILE;
                blend_tile(&mut canvas, canvas_w, dst_x, dst_y, tile);
            }
        }
    }

    ThingRender {
        width_px: canvas_w as u32,
        height_px: canvas_h as u32,
        rgba: canvas,
    }
}

fn read_decoded(
    spr: &mut SprManager,
    spr_path: &str,
    ids: &[u32],
    transparent: bool,
) -> Result<HashMap<u32, Vec<u8>>, String> {
    use rayon::prelude::*;
    let sprites = spr.read_sprites_raw(spr_path, ids)?;
    Ok(sprites
        .into_par_iter()
        .filter(|s| !s.is_empty)
        .map(|s| (s.id, decompress_to_rgba(&s.compressed_pixels, transparent)))
        .collect())
}

pub fn compose_thing_cell(
    spr: &mut SprManager,
    spr_path: &str,
    t: &Thing,
    frame: u32,
    px: u32,
    py: u32,
    pz: u32,
    layer: Option<u32>,
    transparent: bool,
) -> Result<ThingRender, String> {
    let mut ids = Vec::new();
    cell_sprite_ids(t, frame, px, py, pz, layer, &mut ids);
    let decoded = read_decoded(spr, spr_path, &ids, transparent)?;
    Ok(compose_from_decoded(&decoded, t, frame, px, py, pz, layer))
}

/// Nearest-neighbor blit of `src` into `dst`, scaled to fit and centered in a
/// `cell`×`cell` square whose top-left corner is at (cell_x, 0).
fn blit_scaled_into_cell(dst: &mut [u8], dst_w: usize, cell_x: usize, cell: usize, src: &ThingRender) {
    let sw = src.width_px as usize;
    let sh = src.height_px as usize;
    if sw == 0 || sh == 0 {
        return;
    }
    let scale = (cell as f32 / sw as f32).min(cell as f32 / sh as f32);
    let tw = ((sw as f32 * scale) as usize).clamp(1, cell);
    let th = ((sh as f32 * scale) as usize).clamp(1, cell);
    let ox = cell_x + (cell - tw) / 2;
    let oy = (cell - th) / 2;

    for y in 0..th {
        let sy = y * sh / th;
        for x in 0..tw {
            let sx = x * sw / tw;
            let s = (sy * sw + sx) * 4;
            let d = ((oy + y) * dst_w + ox + x) * 4;
            dst[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
}

/// Composes a horizontal strip of thing previews, one `cell`×`cell` square per
/// thing, in the given order. One request per grid row instead of per thing.
pub fn compose_things_row(
    spr: &mut SprManager,
    spr_path: &str,
    things: &[&Thing],
    cell: u32,
    transparent: bool,
) -> Result<ThingRender, String> {
    use rayon::prelude::*;

    let cell = cell as usize;
    let mut all_ids = Vec::new();
    for t in things {
        let (frame, px, py, pz) = preview_pattern(t);
        cell_sprite_ids(t, frame, px, py, pz, None, &mut all_ids);
    }
    let decoded = read_decoded(spr, spr_path, &all_ids, transparent)?;

    let renders: Vec<ThingRender> = things
        .par_iter()
        .map(|t| {
            let (frame, px, py, pz) = preview_pattern(t);
            compose_from_decoded(&decoded, t, frame, px, py, pz, None)
        })
        .collect();

    let row_w = cell * things.len().max(1);
    let mut row = vec![0u8; row_w * cell * 4];
    for (i, render) in renders.iter().enumerate() {
        blit_scaled_into_cell(&mut row, row_w, i * cell, cell, render);
    }

    Ok(ThingRender {
        width_px: row_w as u32,
        height_px: cell as u32,
        rgba: row,
    })
}

/// Default preview cell: first frame, pattern (0,0,0) — except outfits, which
/// face south (pattern_x index 2) when available.
pub fn preview_pattern(t: &Thing) -> (u32, u32, u32, u32) {
    let px = if t.is_outfit && t.pattern_x >= 3 { 2 } else { 0 };
    (0, px, 0, 0)
}

/// Full spritesheet matching OTClient / Object Builder layout:
/// columns = pattern_x × layers (directions/addons left-to-right),
/// rows = pattern_y × frames × pattern_z (animation frames top-to-bottom).
pub fn compose_thing_sheet(
    spr: &mut SprManager,
    spr_path: &str,
    t: &Thing,
    transparent: bool,
) -> Result<ThingRender, String> {
    let cell_w = t.width as usize * TILE;
    let cell_h = t.height as usize * TILE;
    let cols = t.pattern_x as usize * t.layers as usize;
    let rows = t.pattern_y as usize * t.frames as usize * t.pattern_z as usize;
    let sheet_w = cols * cell_w;
    let sheet_h = rows * cell_h;
    if sheet_w * sheet_h > 64 * 1024 * 1024 {
        return Err("Spritesheet would be too large".to_string());
    }

    let mut sheet = vec![0u8; sheet_w * sheet_h * 4];

    for pz in 0..t.pattern_z as u32 {
        for py in 0..t.pattern_y as u32 {
            for px in 0..t.pattern_x as u32 {
                for l in 0..t.layers as u32 {
                    for frame in 0..t.frames as u32 {
                        let cell = compose_thing_cell(spr, spr_path, t, frame, px, py, pz, Some(l), transparent)?;
                        let ox = (px as usize + l as usize * t.pattern_x as usize) * cell_w;
                        let oy = ((pz as usize * t.pattern_y as usize + py as usize) * t.frames as usize
                            + frame as usize)
                            * cell_h;
                        for y in 0..cell_h {
                            let src = y * cell_w * 4;
                            let dst = ((oy + y) * sheet_w + ox) * 4;
                            sheet[dst..dst + cell_w * 4].copy_from_slice(&cell.rgba[src..src + cell_w * 4]);
                        }
                    }
                }
            }
        }
    }

    Ok(ThingRender {
        width_px: sheet_w as u32,
        height_px: sheet_h as u32,
        rgba: sheet,
    })
}

pub fn encode_png(render: &ThingRender) -> Result<Vec<u8>, String> {
    use std::io::Cursor;
    let img = image::RgbaImage::from_raw(render.width_px, render.height_px, render.rgba.clone())
        .ok_or_else(|| "Failed to build image".to_string())?;
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut out, image::ImageOutputFormat::Png)
        .map_err(|e| format!("PNG encode failed: {}", e))?;
    Ok(out.into_inner())
}

// ---------- Manager ----------

pub struct DatManager {
    files: HashMap<String, DatFile>,
}

impl DatManager {
    pub fn new() -> Self {
        Self { files: HashMap::new() }
    }

    pub fn open_file(&mut self, path: String, force_version: Option<u32>) -> Result<DatInfo, String> {
        let dat = open_dat_auto(&path, force_version)?;
        let info = dat.info.clone();
        self.files.insert(path, dat);
        Ok(info)
    }

    pub fn close_file(&mut self, path: &str) {
        self.files.remove(path);
    }

    pub fn file(&self, path: &str) -> Result<&DatFile, String> {
        self.files.get(path).ok_or_else(|| format!("DAT file not open: {}", path))
    }
}

pub type DatManagerState = Arc<Mutex<DatManager>>;
