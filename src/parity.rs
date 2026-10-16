use crate::{
    gameplay::{MovementAction, MovementInput, Player},
    scene::MapId,
};
use bevy::{app::AppExit, prelude::*};
use std::{
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Resource)]
pub struct Telemetry {
    writer: BufWriter<File>,
    started: Instant,
    last_flush: Instant,
    tick: u64,
}

impl Telemetry {
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let now = Instant::now();
        Ok(Self {
            writer: BufWriter::with_capacity(64 * 1024, File::create(path)?),
            started: now,
            last_flush: now,
            tick: 0,
        })
    }

    pub fn record(
        &mut self,
        time: &Time<Fixed>,
        map: MapId,
        physics_hz: f64,
        smooth_physics: bool,
        player: &Player,
        before: Vec3,
        command: MovementInput,
    ) -> io::Result<()> {
        self.tick += 1;
        let now = Instant::now();
        let unix_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        let action = match player.action {
            MovementAction::None => "None",
            MovementAction::Sprint => "Sprint",
            MovementAction::DodgeLeft => "DodgeLeft",
            MovementAction::DodgeRight => "DodgeRight",
            MovementAction::WallJump => "WallJump",
        };
        let displacement = player.feet - before;
        let mut record = serde_json::json!({
            "schema_version": 1,
            "tick": self.tick,
            "fixed_time_ns": time.elapsed().as_nanos() as u64,
            "fixed_dt_ns": time.delta().as_nanos() as u64,
            "dt_seconds": time.delta_secs(),
            "physics_hz": physics_hz,
            "unix_time_ns": unix_time.as_nanos() as u64,
            "monotonic_time_ns": now.duration_since(self.started).as_nanos() as u64,
            "map": map.id(),
            "feet_m": player.feet.to_array(),
            "velocity_mps": player.velocity().to_array(),
            "displacement_m": displacement.to_array(),
            "actual_velocity_mps": (displacement / time.delta_secs()).to_array(),
            "grounded": player.grounded,
            "action": action,
            "action_age_seconds": player.action_age,
            "weapon": format!("{:?}", player.melee.equipped),
            "attack": format!("{:?}", player.melee.attack),
            "attack_age_seconds": player.melee.elapsed,
            "attack_serial": player.melee.serial,
            "wall_jump_phase": format!("{:?}", player.walljump_phase),
            "wall_jump_phase_age_seconds": player.walljump_phase_age,
            "actor_facing": player.actor_facing.to_array(),
            "fall_landing": player.fall_landing,
            "landing_age_seconds": player.landing_age,
            "sp": player.sp,
            "counters": {
                "jumps": player.jumps,
                "dodges": player.dodges,
                "wall_jumps": player.wall_jumps,
                "air_dodges": player.air_dodges,
                "air_sprints": player.air_sprints,
                "respawns": player.respawns,
                "distance_m": player.distance,
                "sprint_distance_m": player.sprint_distance,
            },
            "command": {
                "movement": command.movement.to_array(),
                "jump": command.jump,
                "dodge": command.dodge,
                "dodge_side": command.dodge_side,
                "sprint": command.sprint,
                "crouch": command.crouch,
                "yaw_radians": command.yaw,
                "primary_pressed": command.melee.primary_pressed,
                "primary_held": command.melee.primary_held,
                "primary_released": command.melee.primary_released,
                "secondary_pressed": command.melee.secondary_pressed,
                "equip": command.melee.equip.map(|weapon| format!("{weapon:?}")),
            },
        });
        record["max_sp"] = serde_json::json!(player.max_sp());
        record["skill"] = serde_json::json!(format!("{:?}", player.equipped_skill()));
        record["smooth_physics"] = smooth_physics.into();
        record["health"] = player.health.into();
        record["death_serial"] = player.death_serial.into();
        record["counters"]["wall_jump_failures"] = player.wall_jump_failures.into();
        serde_json::to_writer(&mut self.writer, &record)?;
        self.writer.write_all(b"\n")?;
        if now.duration_since(self.last_flush) >= Duration::from_millis(100) {
            self.writer.flush()?;
            self.last_flush = now;
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub fn flush_on_exit(mut exits: MessageReader<AppExit>, mut telemetry: ResMut<Telemetry>) {
    if exits.read().next().is_some() {
        telemetry.flush().expect("Cannot flush parity telemetry");
    }
}
