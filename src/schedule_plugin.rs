use bevy::prelude::*;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::intern::Interned;

use crate::*;

/*
Game Update loop {
    HandleIo {
        LocalInput,
        Networking,
        Merge Data/Spawn/Despawn
    }


    RollbackSchedule loop {
        Restore if RestoreStates, RestoreInputs if RestoreInputs

        Update,
        PostUpdate,
        Save if SaveStates,
        DespawnNonExistent,
    }
    //after running the RollbackSchedule loop it should hold that current frame == last frame
}
*/

pub struct RollbackSchedulePlugin {
    /// The [`Schedule`] in which rollback processing [`SystemSet`]s will be configured
    pub rollback_processing_schedule: Option<Interned<dyn ScheduleLabel>>,
}

impl Default for RollbackSchedulePlugin {
    fn default() -> Self {
        Self {
            rollback_processing_schedule: Some(Update.intern()), //TODO: this might better be FixedUpdate
            //or at least just for some systems (get local input and run rollback? actually not run rollback as we might need to do rollback after any network input)
        }
    }
}

impl Plugin for RollbackSchedulePlugin {
    fn build(&self, app: &mut App) {
        if let Some(schedule) = self.rollback_processing_schedule {
            app.configure_sets(schedule,
                (
                    RollbackProcessSet::HandleIO,  //TODO: the local input should be probably gathered at fixed frame intervals, just like the RollbackSchedule
                    RollbackProcessSet::RunRollbackSchedule,
                ).chain()
            )
            .add_systems(schedule,run_rollback_schedule_system.in_set(RollbackProcessSet::RunRollbackSchedule));
        }

        app.init_schedule(RollbackSchedule);    // user can app.add_schedule(custom_schedule)

        app.edit_schedule(RollbackSchedule, |schedule| {
            schedule.set_build_settings(bevy::ecs::schedule::ScheduleBuildSettings {
                ambiguity_detection: bevy::ecs::schedule::LogLevel::Error,
                ..default()
            });
        });

        app.configure_sets(RollbackSchedule,(
            (RollbackSet::Restore.run_if(resource_exists::<RestoreStates>), RollbackSet::RestoreInputs.run_if(resource_exists::<RestoreInputs>)),
            RollbackSet::Update,
            RollbackSet::PostUpdate,
            RollbackSet::Save,
            RollbackSet::Despawn,   //maybe Despawn can run at the same time as Save?
        ).chain());

        app.add_systems(RollbackSchedule, 
            (|mut info: ResMut<SnapshotInfo>| info.current += 1)
                .in_set(RollbackSet::PostUpdate));
    }
}

//TODO: split this enum into two enums, one will be pub and second not pub
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum RollbackSet {
    /// Restore the state of rollback entities that are needed to be restored. Restore Resources.
    Restore,
    /// Restore inputs
    RestoreInputs,
    /// Run the game update step
    Update,
    /// Do what needs to be done after updating the game but before saving the state.
    /// Currently it is used to increment the [`SnapshotInfo`] current frame.
    PostUpdate,
    /// Save the state of rollback entities and Resources (not inputs)
    Save,
    /// Despawn rollback entities that use Rollback<Exists> and have no frames with existence
    Despawn,
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
/// This [`SystemSet`] specifies the high level rollback steps. Those are:
/// 1. Getting all the Inputs (current or old/delayed) from all "players"
/// 2. Running the [`RollbackSchedule`] on them. This will replay the past history
/// if old inputs arrived and advance the simulation to the current _now_ frame
pub enum RollbackProcessSet {
    /// Get local input (and probably do network IO)
    HandleIO,
    /// Run the [`RollbackSchedule`]. This will contain the [`run_rollback_schedule_system`] by default
    RunRollbackSchedule,
    //server_only_send_new_spawns? // used by the server to send more stuff after rollback schedule, like new entities
}

#[derive(ScheduleLabel, PartialEq, Eq, Hash, Debug, Clone)]
pub struct RollbackSchedule;





#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
//#[reflect(from_reflect = false)]
pub struct RestoreStates;

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct RestoreInputs;

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct SaveStates;


//inputs will be saved (or modified) outside of RollbackSchedule
//but they will be restored in RollbackSchedule

//maybe I could use trait objects to automatcally do
// app.add_systems(RollbackSchedule, get_rollback_systems::<T>()); for every registered T

//TODO: split this into more systems -> more user configurability and possible simplification
pub fn run_rollback_schedule_system(world: &mut World) {
    //this should NOT contain a loop that will go through all the frames to the last one
    //instead this should be completed by calling this multiple times
    //the reason for wanting this is that more delayed inputs can arive before reaching the last frame
    //TODO: but when run inside Update it will only update 60x per second, but it should update as fast as possible -> fixed by window.present_mode = PresentMode::AutoNoVsync;
    let time = std::time::Instant::now();

    'lop: loop {    //OPTIMIZATION: this loop should not start here but only after finding one modified frame, those checks do not need to be looped
        //if time.elapsed() > std::time::Duration::from_millis(1000/60) {break 'lop}  //TODO: the time here is arbitrary

        let mut info = world.resource_mut::<SnapshotInfo>();
        let last = info.last;
        let snapshots = &mut info.snapshots;

        let oldest = last.saturating_sub(SNAPSHOTS_LEN as u64 - 1);

        let mut modified = None;
        for frame in oldest..=last {
            let index = (frame%SNAPSHOTS_LEN as u64) as usize;
            if snapshots[index].modified {
                assert!(snapshots[index].frame == frame);   //TODO: this should never be possible to fail
                snapshots[index].modified = false;
                modified = Some((index, snapshots[index].frame));
                break;
            }
        }

        if let Some((index, frame)) = modified {
            //print!("*");
            let next_index = (index+1)%SNAPSHOTS_LEN;
            let next_frame = frame+1;

            snapshots[next_index].frame = next_frame;

            if next_frame>last {
                //new update is supposed to happen
                //TODO: perhaps this update should be signaled by different means to eliminate bugs
                //that are caused by accidentally setting the last snapshot's modified=false
                //we are creating a new frame/snapshot, by default it should have modified=false
                snapshots[next_index].modified = false;
                info.last = next_frame;
            }else if next_frame==last {
                //do not set modified to true as that would cause an update even if it should not happen
                //also do not set it to false as that would make an update not happen if it was scheduled to happen but previous state got restored
            }else{
                snapshots[next_index].modified = true;
            }

            //println!("run_rollback_schedule_system frame {frame} index {index} next_index {next_index} last {} current {}",last,info.current);
            
            if frame != info.current {
                info.current = frame;   //the current frame will be used for restoring, TODO: also save the index?
                world.insert_resource(RestoreStates);
            }else{/* the frame that should be restored is already loaded */}
            world.insert_resource(RestoreInputs);
            world.insert_resource(SaveStates);  //the current frame will be used for restoring, but a system will increment the current frame after running RollbackSet::Update

            //println!("run_rollback_schedule_system restoring frame {frame} index {index} input_only {input_only}");

            world.run_schedule(RollbackSchedule);

            world.remove_resource::<RestoreStates>();
            //world.remove_resource::<RestoreInputs>();
            //world.remove_resource::<SaveStates>();
        }else{
            break 'lop
        }
        //break 'lop  //disable the loop for now, it is not needed thanks to window.present_mode = PresentMode::AutoNoVsync; which makes the Update schedule update as fast as possible
    }
    let elapsed = time.elapsed().as_secs_f32();
    if elapsed > 0.01 {
        println!("run_rollback_schedule_system elapsed {elapsed}");
    }
}