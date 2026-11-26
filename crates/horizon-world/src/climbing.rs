//! Registered climbing tags and trapdoor/ladder support with entity state order.
use crate::{
    compression::invalid,
    registry::{BlockRegistry, BlockStateId},
};
use horizon_core::position::BlockPos;
use std::{collections::BTreeSet, fmt, io};
/// Packed per-`BlockStateId` climbing facts (1 byte/state so the whole table fits in L1 cache):
/// - bit 0: `#minecraft:climbable`
/// - bit 1: `#minecraft:can_glide_through`
/// - bits 2..=4: open trapdoor facing (`0 = none`, `1..=4 = north/south/west/east`)
/// - bits 5..=7: ladder facing (`0 = none`, `1..=4 = north/south/west/east`)
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClimbingState(u8);

impl ClimbingState {
    pub const EMPTY: Self = Self(0);

    const CLIMBABLE: u8 = 1 << 0;
    const GLIDE_THROUGH: u8 = 1 << 1;
    const TRAPDOOR_SHIFT: u8 = 2;
    const LADDER_SHIFT: u8 = 5;
    const FACING_MASK: u8 = 0x07;

    #[inline]
    fn new(
        climbable: bool,
        glide_through: bool,
        open_trapdoor: Option<u8>,
        ladder: Option<u8>,
    ) -> Self {
        let mut bits = 0u8;
        if climbable {
            bits |= Self::CLIMBABLE;
        }
        if glide_through {
            bits |= Self::GLIDE_THROUGH;
        }
        if let Some(facing) = open_trapdoor {
            debug_assert!(facing < 4);
            bits |= (facing + 1) << Self::TRAPDOOR_SHIFT;
        }
        if let Some(facing) = ladder {
            debug_assert!(facing < 4);
            bits |= (facing + 1) << Self::LADDER_SHIFT;
        }
        Self(bits)
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn is_climbable(self) -> bool {
        self.0 & Self::CLIMBABLE != 0
    }

    #[inline]
    pub fn is_glide_through(self) -> bool {
        self.0 & Self::GLIDE_THROUGH != 0
    }

    #[inline]
    pub fn open_trapdoor_facing(self) -> Option<u8> {
        let encoded = (self.0 >> Self::TRAPDOOR_SHIFT) & Self::FACING_MASK;
        (encoded != 0).then(|| encoded - 1)
    }

    #[inline]
    pub fn ladder_facing(self) -> Option<u8> {
        let encoded = (self.0 >> Self::LADDER_SHIFT) & Self::FACING_MASK;
        (encoded != 0).then(|| encoded - 1)
    }
}

fn horizontal_facing(value: Option<&str>) -> Option<u8> {
    match value {
        Some("north") => Some(0),
        Some("south") => Some(1),
        Some("west") => Some(2),
        Some("east") => Some(3),
        _ => None,
    }
}
pub struct ClimbingRegistry<'r> {
    registry: &'r BlockRegistry,
    states: Vec<ClimbingState>,
}
impl<'r> ClimbingRegistry<'r> {
    /// `trapdoors` holds `TrapDoorBlock` block names (or tag entries standing in for them); any
    /// entry without a valid `facing` and `open` property is skipped rather than treated as an
    /// error. Rebuild this immutable snapshot when `#minecraft:climbable` or
    /// `#minecraft:can_glide_through` reloads.
    pub fn new(
        registry: &'r BlockRegistry,
        trapdoors: &BTreeSet<String>,
        climbable: &BTreeSet<String>,
        glide_through: &BTreeSet<String>,
    ) -> io::Result<Self> {
        for names in [trapdoors, climbable, glide_through] {
            if names.iter().any(|n| registry.default_state(n).is_none()) {
                return Err(invalid("unknown climbing block member"));
            }
        }
        let mut states = Vec::with_capacity(registry.len());
        for i in 0..registry.len() {
            let s = registry.state(BlockStateId(i as u32)).unwrap();
            let facing = horizontal_facing(s.properties.get("facing").map(String::as_str));
            // Vanilla tests `instanceof TrapDoorBlock`; the tag only stands in for it. A mod can
            // list blocks that are not trapdoors (no `open` or `facing`): those never climb.
            let open_trapdoor = if trapdoors.contains(&s.name) {
                match (facing, s.properties.get("open").map(String::as_str)) {
                    (Some(f), Some("true")) => Some(f),
                    _ => None,
                }
            } else {
                None
            };
            let ladder = if s.name == "minecraft:ladder" {
                Some(facing.ok_or_else(|| invalid("invalid climbing facing"))?)
            } else {
                None
            };
            states.push(ClimbingState::new(
                climbable.contains(&s.name),
                glide_through.contains(&s.name),
                open_trapdoor,
                ladder,
            ));
        }
        Ok(Self { registry, states })
    }
    pub fn registry(&self) -> &'r BlockRegistry {
        self.registry
    }
    #[inline]
    pub fn get(&self, id: BlockStateId) -> Option<ClimbingState> {
        self.states.get(id.0 as usize).copied()
    }
    #[inline]
    fn state(&self, id: BlockStateId) -> io::Result<ClimbingState> {
        self.get(id)
            .ok_or_else(|| invalid("climbing state outside registry"))
    }
    #[inline]
    pub fn is_climbable_tag(&self, id: BlockStateId) -> io::Result<bool> {
        Ok(self.state(id)?.is_climbable())
    }
}
pub trait ClimbingWorld {
    type Error;
    fn is_spectator(&mut self) -> Result<bool, Self::Error>;
    fn block_position(&mut self) -> Result<BlockPos, Self::Error>;
    fn in_block_state(&mut self) -> Result<BlockStateId, Self::Error>;
    fn is_fall_flying(&mut self) -> Result<bool, Self::Error>;
    fn block_state(&mut self, position: BlockPos) -> Result<BlockStateId, Self::Error>;
    fn set_last_climbable_position(&mut self, position: BlockPos);
    /// `IBlockExtension.isLadder` for the block `state` at `position` the entity stands in, once
    /// the `#minecraft:climbable` tag answered `tagged`: the answer to use. The default keeps it.
    fn adjust_ladder(&mut self, _position: BlockPos, _state: BlockStateId, tagged: bool) -> bool {
        tagged
    }
    /// `IBlockExtension.makesOpenTrapdoorAboveClimbable` for the block `state` at `position`
    /// under the open trapdoor `trapdoor` the entity stands in, once vanilla answered `ladder` (a
    /// ladder facing the trapdoor's way): the answer to use. The default keeps it.
    fn adjust_trapdoor_ladder(
        &mut self,
        _position: BlockPos,
        _state: BlockStateId,
        _trapdoor: BlockStateId,
        ladder: bool,
    ) -> bool {
        ladder
    }
}
#[derive(Debug)]
pub enum ClimbingError<E> {
    Data(io::Error),
    World(E),
}
impl<E: fmt::Display> fmt::Display for ClimbingError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(e) => e.fmt(f),
            Self::World(e) => e.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for ClimbingError<E> {}
/// Failed/negative queries retain the existing last-climbable position. Only a
/// positive result overwrites it, using the position captured before state reads.
pub fn on_climbable<W: ClimbingWorld>(
    world: &mut W,
    registry: &ClimbingRegistry<'_>,
) -> Result<bool, ClimbingError<W::Error>> {
    use ClimbingError::{Data, World};
    if world.is_spectator().map_err(World)? {
        return Ok(false);
    }
    let position = world.block_position().map_err(World)?;
    let in_state = world.in_block_state().map_err(World)?;
    let state = registry.state(in_state).map_err(Data)?;
    if world.is_fall_flying().map_err(World)? && state.is_glide_through() {
        return Ok(false);
    }
    let climbable = if world.adjust_ladder(position, in_state, state.is_climbable()) {
        true
    } else if let Some(facing) = state.open_trapdoor_facing() {
        let below = BlockPos::new(position.x, position.y.wrapping_sub(1), position.z);
        let below_state = world.block_state(below).map_err(World)?;
        let ladder = registry.state(below_state).map_err(Data)?.ladder_facing() == Some(facing);
        world.adjust_trapdoor_ladder(below, below_state, in_state, ladder)
    } else {
        false
    };
    if climbable {
        world.set_last_climbable_position(position);
    }
    Ok(climbable)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    const AIR: BlockStateId = BlockStateId(0);
    const LADDER_NORTH: BlockStateId = BlockStateId(1);
    const OPEN_TRAPDOOR_NORTH: BlockStateId = BlockStateId(2);
    const CLOSED_TRAPDOOR_NORTH: BlockStateId = BlockStateId(3);
    const BRIDGE_NO_OPEN: BlockStateId = BlockStateId(4);
    const PIER_NO_PROPERTIES: BlockStateId = BlockStateId(5);
    const OPEN_NO_FACING: BlockStateId = BlockStateId(6);

    /// A trapdoor with `facing` and `open`, and three blocks a mod put in the trapdoor tag that
    /// are not trapdoors: no `open` (Macaw's rail bridge), no properties (its pier), and no
    /// `facing`.
    fn registry() -> BlockRegistry {
        let json = br#"{"defaults": {"minecraft:air": 0, "minecraft:ladder": 1,
            "minecraft:oak_trapdoor": 3, "mod:bridge": 4, "mod:pier": 5, "mod:odd": 6},
            "states": [
              ["minecraft:air", true, false, "x", "minecraft:air"],
              ["minecraft:ladder", false, false, "x", {"id": "minecraft:ladder", "properties": {"facing": "north"}}],
              ["minecraft:oak_trapdoor", false, false, "x", {"id": "minecraft:oak_trapdoor", "properties": {"facing": "north", "open": "true"}}],
              ["minecraft:oak_trapdoor", false, false, "x", {"id": "minecraft:oak_trapdoor", "properties": {"facing": "north", "open": "false"}}],
              ["mod:bridge", false, false, "x", {"id": "mod:bridge", "properties": {"facing": "north", "powered": "false"}}],
              ["mod:pier", false, false, "x", "mod:pier"],
              ["mod:odd", false, false, "x", {"id": "mod:odd", "properties": {"open": "true"}}]
            ]}"#;
        BlockRegistry::from_facts_json_with_flags(&json[..])
            .unwrap()
            .0
    }

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|n| (*n).to_owned()).collect()
    }

    /// The entity stands in `inside`, with `below` under its block.
    struct World {
        inside: BlockStateId,
        below: BlockStateId,
        last: Option<BlockPos>,
        /// A part's `is_ladder` answer for one state, with the tag's answer it was asked after.
        ladder: Option<(BlockStateId, bool)>,
        asked: Vec<(BlockStateId, bool)>,
        /// A part's answer for the block under an open trapdoor, and what it was asked: that
        /// block's position and state, the trapdoor and vanilla's answer.
        below_answer: Option<bool>,
        below_asked: Vec<(BlockPos, BlockStateId, BlockStateId, bool)>,
    }
    impl ClimbingWorld for World {
        type Error = Infallible;
        fn is_spectator(&mut self) -> Result<bool, Infallible> {
            Ok(false)
        }
        fn block_position(&mut self) -> Result<BlockPos, Infallible> {
            Ok(BlockPos::new(1, 2, 3))
        }
        fn in_block_state(&mut self) -> Result<BlockStateId, Infallible> {
            Ok(self.inside)
        }
        fn is_fall_flying(&mut self) -> Result<bool, Infallible> {
            Ok(false)
        }
        fn block_state(&mut self, _: BlockPos) -> Result<BlockStateId, Infallible> {
            Ok(self.below)
        }
        fn set_last_climbable_position(&mut self, position: BlockPos) {
            self.last = Some(position);
        }
        fn adjust_ladder(&mut self, _: BlockPos, state: BlockStateId, tagged: bool) -> bool {
            self.asked.push((state, tagged));
            match self.ladder {
                Some((answered, ladder)) if answered == state => ladder,
                _ => tagged,
            }
        }
        fn adjust_trapdoor_ladder(
            &mut self,
            position: BlockPos,
            state: BlockStateId,
            trapdoor: BlockStateId,
            ladder: bool,
        ) -> bool {
            self.below_asked.push((position, state, trapdoor, ladder));
            self.below_answer.unwrap_or(ladder)
        }
    }

    fn climbs(registry: &ClimbingRegistry<'_>, inside: BlockStateId, below: BlockStateId) -> bool {
        climbs_with(registry, inside, below, None)
    }

    fn climbs_with(
        registry: &ClimbingRegistry<'_>,
        inside: BlockStateId,
        below: BlockStateId,
        ladder: Option<(BlockStateId, bool)>,
    ) -> bool {
        let mut world = World {
            inside,
            below,
            last: None,
            ladder,
            asked: Vec::new(),
            below_answer: None,
            below_asked: Vec::new(),
        };
        let result = on_climbable(&mut world, registry).unwrap();
        assert_eq!(result, world.last.is_some());
        result
    }

    #[test]
    fn tagged_blocks_without_trapdoor_properties_do_not_fail_the_binding() {
        let registry = registry();
        let tagged = names(&[
            "minecraft:oak_trapdoor",
            "mod:bridge",
            "mod:pier",
            "mod:odd",
        ]);
        let empty = BTreeSet::new();
        let climbing = ClimbingRegistry::new(&registry, &tagged, &empty, &empty)
            .expect("a tag member that is not a trapdoor must not stop the start");

        // The real trapdoor still climbs, and only open over a ladder facing the same way.
        assert!(climbs(&climbing, OPEN_TRAPDOOR_NORTH, LADDER_NORTH));
        assert!(!climbs(&climbing, CLOSED_TRAPDOOR_NORTH, LADDER_NORTH));
        assert!(!climbs(&climbing, OPEN_TRAPDOOR_NORTH, AIR));
        // Vanilla's `instanceof TrapDoorBlock` is false for these, so they never climb.
        for id in [BRIDGE_NO_OPEN, PIER_NO_PROPERTIES, OPEN_NO_FACING] {
            assert!(!climbs(&climbing, id, LADDER_NORTH));
        }
    }

    #[test]
    fn a_missing_open_property_and_a_missing_facing_each_count_as_not_a_trapdoor() {
        let registry = registry();
        let empty = BTreeSet::new();
        // Alone, so each case is tested without the other: `mod:bridge` has `facing` but no
        // `open`; `mod:odd` has `open` but no `facing`; `mod:pier` has neither.
        for name in ["mod:bridge", "mod:odd", "mod:pier"] {
            let tagged = names(&[name]);
            assert!(ClimbingRegistry::new(&registry, &tagged, &empty, &empty).is_ok());
        }
    }

    #[test]
    fn a_blocks_ladder_answer_replaces_the_tag() {
        let registry = registry();
        // Only the mod's pier is tagged `climbable` here; the ladder and the air are not.
        let climbable = names(&["mod:pier"]);
        let empty = BTreeSet::new();
        let climbing = ClimbingRegistry::new(&registry, &empty, &climbable, &empty).unwrap();

        // Default: the tag decides, as before (the block asked is the one the entity stands in).
        assert!(climbs(&climbing, PIER_NO_PROPERTIES, AIR));
        assert!(!climbs(&climbing, LADDER_NORTH, AIR));
        assert!(!climbs(&climbing, AIR, AIR));
        let mut world = World {
            inside: PIER_NO_PROPERTIES,
            below: AIR,
            last: None,
            ladder: None,
            asked: Vec::new(),
            below_answer: None,
            below_asked: Vec::new(),
        };
        on_climbable(&mut world, &climbing).unwrap();
        assert_eq!(world.asked, [(PIER_NO_PROPERTIES, true)]);

        // Allow: an untagged block climbs, and the block is remembered.
        assert!(climbs_with(&climbing, AIR, AIR, Some((AIR, true))));
        assert!(climbs_with(
            &climbing,
            LADDER_NORTH,
            AIR,
            Some((LADDER_NORTH, true))
        ));
        // Deny: a tagged block does not climb, and nothing is remembered.
        assert!(!climbs_with(
            &climbing,
            PIER_NO_PROPERTIES,
            AIR,
            Some((PIER_NO_PROPERTIES, false))
        ));
        // An answer for another state changes nothing.
        assert!(climbs_with(
            &climbing,
            PIER_NO_PROPERTIES,
            AIR,
            Some((AIR, false))
        ));
        assert!(!climbs_with(
            &climbing,
            AIR,
            AIR,
            Some((LADDER_NORTH, true))
        ));
    }

    #[test]
    fn a_denied_ladder_answer_leaves_the_trapdoor_over_a_ladder() {
        let registry = registry();
        let trapdoors = names(&["minecraft:oak_trapdoor"]);
        let climbable = names(&["minecraft:oak_trapdoor"]);
        let empty = BTreeSet::new();
        let climbing = ClimbingRegistry::new(&registry, &trapdoors, &climbable, &empty).unwrap();
        // Vanilla's `else if` falls to the trapdoor rule when the block is not a ladder: an open
        // trapdoor over a matching ladder still climbs when its own answer is no.
        assert!(climbs_with(
            &climbing,
            OPEN_TRAPDOOR_NORTH,
            LADDER_NORTH,
            Some((OPEN_TRAPDOOR_NORTH, false))
        ));
        assert!(!climbs_with(
            &climbing,
            OPEN_TRAPDOOR_NORTH,
            AIR,
            Some((OPEN_TRAPDOOR_NORTH, false))
        ));
    }

    #[test]
    fn a_part_on_the_block_below_decides_the_trapdoor_ladder() {
        let registry = registry();
        let trapdoors = names(&["minecraft:oak_trapdoor"]);
        let empty = BTreeSet::new();
        let climbing = ClimbingRegistry::new(&registry, &trapdoors, &empty, &empty).unwrap();
        let world = |inside, below, answer| World {
            inside,
            below,
            last: None,
            ladder: None,
            asked: Vec::new(),
            below_answer: answer,
            below_asked: Vec::new(),
        };
        let under = BlockPos::new(1, 1, 3);
        // Air under an open trapdoor: vanilla says no, the part's yes climbs. The block below
        // is asked at its own position, with the trapdoor above it.
        let mut yes = world(OPEN_TRAPDOOR_NORTH, AIR, Some(true));
        assert!(on_climbable(&mut yes, &climbing).unwrap());
        assert_eq!(yes.below_asked, [(under, AIR, OPEN_TRAPDOOR_NORTH, false)]);
        assert_eq!(yes.last, Some(BlockPos::new(1, 2, 3)));
        // A ladder facing the trapdoor's way: vanilla says yes, the part's no does not climb.
        let mut no = world(OPEN_TRAPDOOR_NORTH, LADDER_NORTH, Some(false));
        assert!(!on_climbable(&mut no, &climbing).unwrap());
        assert_eq!(
            no.below_asked,
            [(under, LADDER_NORTH, OPEN_TRAPDOOR_NORTH, true)]
        );
        assert_eq!(no.last, None);
        // No answer keeps vanilla's.
        let mut kept = world(OPEN_TRAPDOOR_NORTH, LADDER_NORTH, None);
        assert!(on_climbable(&mut kept, &climbing).unwrap());
        // A closed trapdoor asks the block below nothing.
        let mut closed = world(CLOSED_TRAPDOOR_NORTH, AIR, Some(true));
        assert!(!on_climbable(&mut closed, &climbing).unwrap());
        assert!(closed.below_asked.is_empty());
    }

    #[test]
    fn packed_climbing_state_round_trips_all_flags_and_facings() {
        assert_eq!(std::mem::size_of::<ClimbingState>(), 1);
        assert!(ClimbingState::EMPTY.is_empty());
        assert!(!ClimbingState::EMPTY.is_climbable());
        assert!(!ClimbingState::EMPTY.is_glide_through());
        assert_eq!(ClimbingState::EMPTY.open_trapdoor_facing(), None);
        assert_eq!(ClimbingState::EMPTY.ladder_facing(), None);

        for climbable in [false, true] {
            for glide in [false, true] {
                for trapdoor in [None, Some(0), Some(1), Some(2), Some(3)] {
                    for ladder in [None, Some(0), Some(1), Some(2), Some(3)] {
                        let s = ClimbingState::new(climbable, glide, trapdoor, ladder);
                        assert_eq!(s.is_climbable(), climbable);
                        assert_eq!(s.is_glide_through(), glide);
                        assert_eq!(s.open_trapdoor_facing(), trapdoor);
                        assert_eq!(s.ladder_facing(), ladder);
                        assert_eq!(
                            s.is_empty(),
                            !climbable && !glide && trapdoor.is_none() && ladder.is_none()
                        );
                    }
                }
            }
        }
    }
}
