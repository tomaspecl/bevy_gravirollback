use bevy::prelude::*;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::intern::Interned;

use crate::*;

/// Restore the state of rollback entities that are needed to be restored. Restore Resources.
/// For restoring inputs use [`RollbackUpdateSet::LoadInputs`] inside the [`RollbackUpdate`] [`Schedule`].
#[derive(ScheduleLabel, Hash, Debug, PartialEq, Eq, Clone)]
pub struct RollbackRestore;

/// Run the game update step, use [`RollbackUpdateSet`] to put game logic into [`RollbackUpdateSet::Update`].
#[derive(ScheduleLabel, Hash, Debug, PartialEq, Eq, Clone)]
pub struct RollbackUpdate;

/// This [`SystemSet`] normally runs in [`RollbackUpdate`] [`Schedule`]
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum RollbackUpdateSet {
    /// Load inputs for the current [`Frame`].
    /// It could be in a form of a [`Resource`] called `Inputs` (defined by you), snapshots of which are stored in [`Rollback<Inputs>`].
    /// For loading the correct `Inputs` for the current [`Frame`], you could use [`bevy_gravirollback::systems::restore_resource::<Inputs>`].
    LoadInputs,
    /// Update the game state based on the current state and current inputs.
    /// This step should be deterministic, otherwise unexpected behaviour may occur (such as unexpected change of the state on rollback and the following resimulation).
    /// 
    /// Note that [`Frame`] should be incremented after running the game state update and before running the [`RollbackSave`] [`Schedule`].
    //TODO: give more info, RollbackSchedulePlugin, run_rollback_schedule_system, etc...
    Update,
}

/// The [`Frame`] that should be reached by running the [`RollbackUpdate`] [`Schedule`].
/// This should be modified outside of the rollback schedules, rollback systems, etc...
#[derive(Resource, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Resource)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct WantedFrame(pub u64);

/// Save the state of rollback entities and Resources (not inputs).
/// The frame that is being saved is in [`Frame`] [`Resource`].
/// 
#[derive(ScheduleLabel, Hash, Debug, PartialEq, Eq, Clone)]
pub struct RollbackSave;


/// This [`SystemSet`] specifies the high level rollback steps. Those are:
/// 1. Getting all the Inputs (current or old/delayed) from all "players"
/// 2. Running the [`RollbackSchedule`] on them. This will replay the past history
/// if old inputs arrived and advance the simulation to the current _now_ frame
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum RollbackProcessSet {
    /// Get local input (and probably do network IO)
    HandleIO,
    /// Run the [`RollbackSchedule`]. This will contain the [`run_rollback_schedule_system`] by default
    RunRollbackSchedule,
}

/// Config for rollback_update_system
#[derive(Resource, Reflect, Default, Clone, Copy)]
#[reflect(Resource)]
pub struct RollbackUpdateConfig {
    /// How many consecutive updates are allowed inside single execution of [`rollback_update_system`].
    /// Value `0` means infinite.
    pub max_update_loops: u32,
}

pub struct RollbackSchedulePlugin<const LEN: usize> {
    /// The [`Schedule`] in which rollback processing [`SystemSet`]s will be configured
    pub rollback_processing_schedule: Option<Interned<dyn ScheduleLabel>>,
}

impl<const LEN: usize> Default for RollbackSchedulePlugin<LEN> {
    fn default() -> Self {
        Self {
            rollback_processing_schedule: Some(Update.intern()), //TODO: this might better be FixedUpdate
            //or at least just for some systems (get local input and run rollback? actually not run rollback as we might need to do rollback after any network input)
        }
    }
}

impl<const LEN: usize> Plugin for RollbackSchedulePlugin<LEN> {
    fn build(&self, app: &mut App) {
        if let Some(schedule) = self.rollback_processing_schedule {
            app.configure_sets(schedule,
                (
                    RollbackProcessSet::HandleIO,  //TODO: the local input should be probably gathered at fixed frame intervals, just like the RollbackSchedule
                    RollbackProcessSet::RunRollbackSchedule,
                ).chain()
            )
            .add_systems(schedule,(
                rollback_restore_system::<LEN>,
                rollback_update_system::<LEN>,
            ).chain().in_set(RollbackProcessSet::RunRollbackSchedule));
        }

        app
        .init_resource::<WantedFrame>()
        .init_resource::<RollbackUpdateConfig>()
        .init_schedule(RollbackRestore)
        .init_schedule(RollbackUpdate)
        .init_schedule(RollbackSave)
        .configure_sets(RollbackUpdate, (RollbackUpdateSet::LoadInputs, RollbackUpdateSet::Update).chain())

        .add_systems(RollbackSave, new_frame_save_system::<LEN>)

        //.edit_schedule(RollbackUpdate, |schedule| {
        //    schedule.set_build_settings(bevy::ecs::schedule::ScheduleBuildSettings {
        //        ambiguity_detection: bevy::ecs::schedule::LogLevel::Error,
        //        ..default()
        //    });
        //})
        ;
    }
}

pub fn rollback_restore_system<const LEN: usize>(world: &mut World) {
    let current_frame = world.resource::<Frame>().0;
    let last_frame = world.resource::<LastFrame>().0;
    let modified = world.resource::<Rollback<Modified, LEN>>();
    let frames = world.resource::<Rollback<Frame, LEN>>();

    let oldest_frame = last_frame.saturating_sub(LEN as u64 - 1);

    assert!(current_frame <= last_frame, "perhaps rollback_save_system was not run immediately after rollback_update_system");
    for frame in oldest_frame..current_frame { //exclude the current frame, we do not need to restore the current frame as it is already loaded
        let index = index::<LEN>(frame);
        if modified[index].0 {
            assert!(frames[index].0 == frame);   //TODO: this should never be possible to fail
            
            //restore this (past) frame
            world.resource_mut::<Frame>().0 = frame;
            //world.resource_mut::<Index<LEN>>().0 = index; //maybe in the future as an optimization, now I want simplicity
            world.run_schedule(RollbackRestore);
            return
        }
    }
}

pub fn rollback_update_system<const LEN: usize>(world: &mut World) {
    let time = std::time::Instant::now();

    let mut current_frame = world.resource::<Frame>().0;
    let wanted_frame = world.resource::<WantedFrame>().0;

    let rollback_update_config = world.resource::<RollbackUpdateConfig>().clone();
    let mut i = 0u32;
    loop {
        if wanted_frame > current_frame {
            //run the update to move to the next frame
            let current_index = index::<LEN>(current_frame);
            let mut modified = world.resource_mut::<Rollback<Modified, LEN>>();
            modified[current_index].0 = false;  //if this frame was modified, by resimulating it, we resolved any changes that could be there
            
            world.run_schedule(RollbackUpdate);
            
            current_frame += 1;     //now we are in the next frame, LastFrame will be updated accordingly in rollback_save_system in case Frame > LastFrame
            world.resource_mut::<Frame>().0 = current_frame;

            rollback_save_system::<LEN>(world);
        }else{
            break //update is not wanted
        }

        i += 1;

        if rollback_update_config.max_update_loops == 0 {
            continue    //no limit is set
        }

        if i >= rollback_update_config.max_update_loops {
            break   //the maximum number of allowed updates happened
        }
    }

    //TODO: debugging should be configurable in rollback_update_config
    let elapsed = time.elapsed().as_secs_f32();
    if elapsed > 0.01 {
        println!("run_rollback_schedule_system elapsed {elapsed}");
    }
}

/// under default behaviour this system will be called like a function from inside [`rollback_update_system`] after an update,
/// instead of running in a [`Schedule`].
pub fn rollback_save_system<const LEN: usize>(world: &mut World) {
    //the RollbackSave Schedule is being run first so that the systems inside it can detect when current_frame > last_frame, if they need it
    world.run_schedule(RollbackSave);

    let current_frame = world.resource::<Frame>().0;
    let last_frame = &mut world.resource_mut::<LastFrame>().0;

    if current_frame > *last_frame {
        assert!(current_frame == *last_frame+1);
        *last_frame = current_frame;
    }
}

pub fn new_frame_save_system<const LEN: usize>(
    current_frame: Res<Frame>,
    last_frame: Res<LastFrame>,
    mut frames: ResMut<Rollback<Frame, LEN>>,
    mut modified: ResMut<Rollback<Modified, LEN>>
) {
    if current_frame.0 > last_frame.0 {
        let current_index = crate::index::<LEN>(current_frame.0);
        //we are saving a new frame, by default it should have modified=false
        frames[current_index] = *current_frame;
        modified[current_index].0 = false;
    }
}