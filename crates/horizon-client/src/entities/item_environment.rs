use super::*;

impl ItemEntity {
    /// `Entity.applyEffectsFromBlocks` after the move: lava, fire and cactus
    /// hurt the item, then water or rain (`in_rain`, `Entity.isInRain`) puts it
    /// out. Each hit posts `entity_damage` at the item's position, and the hit
    /// that destroys it ends the pass. The burning of `Entity.baseTick` is
    /// [`ItemEntity::burning_tick`]'s.
    pub fn environment_tick(
        &mut self,
        mut world: &World,
        facts: &BlockFacts,
        registry: &horizon_world::registry::BlockRegistry,
        can_be_hurt_by: &dyn Fn(&ItemStack, &str) -> bool,
        in_rain: bool,
    ) -> ItemEnvironmentTick {
        self.environment_tick_inner(
            &mut world,
            facts,
            registry,
            can_be_hurt_by,
            in_rain,
            |_, _| false,
        )
    }
    /// As `environment_tick`, allowing the caller to decide each actual damage attempt.
    pub fn environment_tick_with_hook(
        &mut self,
        mut world: &mut World,
        facts: &BlockFacts,
        registry: &horizon_world::registry::BlockRegistry,
        can_be_hurt_by: &dyn Fn(&ItemStack, &str) -> bool,
        in_rain: bool,
        mut invulnerable: impl FnMut(&mut World, &str) -> bool,
    ) -> ItemEnvironmentTick {
        self.environment_tick_inner(
            &mut world,
            facts,
            registry,
            can_be_hurt_by,
            in_rain,
            |world, damage| invulnerable(world, damage),
        )
    }
    fn environment_tick_inner<W: std::ops::Deref<Target = World>>(
        &mut self,
        world: &mut W,
        facts: &BlockFacts,
        _registry: &horizon_world::registry::BlockRegistry,
        can_be_hurt_by: &dyn Fn(&ItemStack, &str) -> bool,
        in_rain: bool,
        mut invulnerable: impl FnMut(&mut W, &str) -> bool,
    ) -> ItemEnvironmentTick {
        let fluid = self.fluid_at_feet(world, facts);
        let fire_immune = !can_be_hurt_by(&self.stack, "minecraft:in_fire");
        let mut hits = ItemHits::default();
        if fire_immune {
            self.fire_ticks = 0;
        }
        // `Entity.applyEffectsFromBlocks` snapshots `isOnFire` before the block
        // contacts, which may ignite the item this tick, then puts it out in
        // water or rain; the hiss compares the two.
        let was_on_fire = self.fire_ticks > 0;
        let blocks_apply = self.health > 0;
        let mut lava_hurt = false;
        let bounds = self.bounds();
        // Pre-query the section hazard mask once with the wider `1e-5_f32` inset so one
        // cached palette read gates lava cauldron, fire, and cactus checks together.
        // If an `invulnerable` hook runs, fall back to re-checking in case it mutates `world`.
        use crate::blocks::hazard;
        let epsilon = f64::from(1e-5_f32);
        let box_hazards = crate::motion::environment::closed_cell_bounds(
            bounds.deflate(Vec3::new(epsilon, epsilon, epsilon)),
        )
        .map_or(0, |(lo, hi)| world.hazard_mask(lo, hi, facts));
        let mut world_touched = false;
        // LavaCauldronBlock.entityInside applies LAVA_IGNITE and then lavaHurt.
        // A cauldron is not a lava fluid, and removed items get no block effects.
        if self.health > 0 && !fire_immune {
            let cauldrons = if box_hazards & hazard::LAVA_CAULDRON != 0 {
                lava_cauldron_contacts(world, facts, bounds)
            } else {
                0
            };
            let contacts = usize::from(fluid == Fluid::Lava) + cauldrons;
            if contacts > 0 {
                self.fire_ticks = self.fire_ticks.max(300);
            }
            // StepBasedCollector coalesces ignition but keeps each lavaHurt.
            for _ in 0..contacts {
                if self.health <= 0 {
                    break;
                }
                if can_be_hurt_by(&self.stack, "minecraft:lava") {
                    world_touched = true;
                    if !invulnerable(world, "minecraft:lava") {
                        hits.hit(self, 4);
                        lava_hurt = true;
                    }
                }
            }
        }
        if !fire_immune && (world_touched || box_hazards & hazard::ANY_FIRE != 0) {
            let fire_damage = crate::motion::environment::fire_contact(world, facts, bounds);
            if fire_damage > 0.0 {
                self.fire_ticks = self.fire_ticks.max(160);
                if can_be_hurt_by(&self.stack, "minecraft:in_fire") {
                    world_touched = true;
                    if !invulnerable(world, "minecraft:in_fire") {
                        hits.hit(self, fire_damage as i32);
                    }
                }
            }
        }
        // CactusBlock.entityInside: `entity.hurt(cactus, 1.0)`, a point a tick
        // (an item has no hurt cooldown), unless `canBeHurtBy` skips it.
        if (world_touched || box_hazards & hazard::CACTUS != 0)
            && crate::motion::environment::cactus_contact(world, facts, bounds)
            && can_be_hurt_by(&self.stack, "minecraft:cactus")
            && !invulnerable(world, "minecraft:cactus")
        {
            hits.hit(self, 1);
        }
        let extinguished =
            blocks_apply && self.extinguish_if_wet(fluid == Fluid::Water, in_rain, was_on_fire);
        ItemEnvironmentTick {
            alive: self.health > 0,
            lava_hurt,
            extinguished,
            spilled: hits.spilled,
            damage_events: hits.events,
        }
    }
}
