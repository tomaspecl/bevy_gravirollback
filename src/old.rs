use bevy::prelude::*;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::utils::{HashMap, Entry};
use std::collections::VecDeque;

//the old stuff
#[derive(Resource, Default, Clone)]
pub struct Inputs(pub HashMap<Player, Input>);  //this should be defined by user

#[derive(Clone)]
struct Player;  //stub

/// This will be broadcast to everyone
#[derive(Clone)]
struct Input;

/*
ROLLBACK

it should be possible to have more types of States, for different types of entities with different data and components

*/

pub struct RollbackPlugin {
    /// The [`Schedule`] in which rollback processing [`SystemSet`]s will be configured
    pub rollback_processing_schedule: Option<Box<dyn ScheduleLabel>>,
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
/// This [`SystemSet`] specifies the high level rollback steps. Those are:
/// 1. Getting all the [`Inputs`] (current or old/delayed) from all "players"
/// 2. Running the [`RollbackSchedule`] on them. This will replay the past history
/// if old inputs arrived and advance the simulation to the current _now_ frame
pub enum RollbackProcessSet {
    /// Get local input (and probably do network IO)
    ProcessIO,
    /// Run the [`RollbackSchedule`]. This will contain the [`run_rollback_schedule_system`] by default
    RunRollbackSchedule,
    //server_only_send_new_spawns? // used by the server to send more stuff after rollback schedule, like new entities
}

#[derive(ScheduleLabel, PartialEq, Eq, Hash, Debug, Clone)]
pub struct RollbackSchedule;

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum RollbackSet {
    PrepareRestore,
    RestoreState,
    Update,
    SaveState,
}

/// This resource will signal which Snapshot has to be restored in the current RollbackSchedule update
#[derive(Resource)]
pub struct SnapshotToRestore<S,I>(Snapshot<S,I>);

/// This resource informs about the current loaded Snapshot.
/// The inner value is the index of the Snapshot in the Snapshots storage.
/// Index 0 is the current (newest) snapshot TODO: check this indexing (is it not index 1 instead?)
pub struct CurrentSnapshotIndex(usize);

/// This resource will be true when the current RollbackSchedule run has not restored a past Snapshot yet
/// TODO: is this actualy needed when we have RollbackRestoreFlag?
//#[derive(Resource)]
//pub struct RollbackFirstLoop(bool);

#[derive(Component, PartialEq, Eq, Hash, Clone)]
struct Rollback(u64);

/// This will be only sent to the server, the server will not broadcast it to everyone
/// This can be used for spawning requests for example
struct ServerOnlyInput;

/// All kinds of settings
//TODO: maybe this should be inside [`Snapshots`]
#[derive(Resource)]
struct SnapshotsConfig {
    /// The length of the snapshot storage buffer. How long will the stored history be.
    buffer_len: usize,
}

//TODO: idea: make the [`Snapshots`] a component, and the RollbackSchedule too -> then you can put
// a minigame inside your game and make the minigame "object/entity" have its own seperate rollback system

/// Snapshot storage
#[derive(Resource, Clone)]
pub struct Snapshots<S,I> {
    /// Buffer of Snapshots. The last (current) Snapshot is in the front.
    /// Old snapshots get overwritten by new ones.
    pub buffer: VecDeque<Snapshot<S,I>>,
    /// The last Snapshot (frame) number - current frame
    pub last_frame: u64,
    /// What the last_frame should be after running the Update
    pub target_frame: u64,
    /// The last Snapshot (frame) time in miliseconds since UNIX_EPOCH
    pub last_frame_time: u128,  //TODO: isnt this overkill? u64 should be enough
}

/// Snapshot of one game frame
#[derive(Resource, Clone)]
struct Snapshot<S,I> {  //TODO: add custom user data as generic parameter
    /// States of all Rollback entities
    pub states: HashMap<Rollback,S>,
    /// Inputs of this frame, they will influence the next frame
    pub inputs: I,
    /// This flag signals to the system that this Snapshot was modified and needs to be recomputed
    pub modified: bool,
    //TODO: time should probably not go here
    ///// The time of creation of this Snapshot
    //pub time: u128,     //TODO: u64 should be enough, also is this needed?
}

/// Saved state of one Rollback entity
#[derive(Clone)]
pub struct State<S> {
    /// When false, the Client will recompute this State when past Inputs get updated.
    /// When the Server sends corrections of the State to the Client, this flag will
    /// be set such that it wont be overwritten by the Client.
    pub fixed: bool,    //TODO: should I have this here by default? if not Snapshot can just have states: HashMap<Rollback, S>,
    // no this should not contain "fixed". This feature should be optional. User could impl a trait on their own State and then
    // use their own state in a default system which uses this "fixed" flag provided by the trait

    ///The saved state
    pub state: S,
}

/// This [`Resource`] will be put into the [`World`] at the start of the
/// [`RollbackSchedule`] to signal that this state should be restored by
/// overwriting the current state
#[derive(Resource)]
pub struct StateToRestore<S> {
    to_spawn: HashMap<Rollback,S>,
    to_overwrite: HashMap<Rollback,S>,
    //to_delete: Vec<Entity>,
}

//this will run at the start of RollbackSchedule at RollbackSet::PrepareRestore
//this should not need to be implemented by user
//TODO: make variant which autodeletes
pub fn prepare_to_restore<S: 'static + Send + Sync + Clone, I: 'static + Send + Sync>(
    snapshot: Res<SnapshotToRestore<S, I>>,
    mut state_to_restore: ResMut<StateToRestore<S>>,
    query: Query<(Entity, &Rollback)>,
) {
    //go through the current Snapshot specified for restoring and
    // sort the Rollback entities into two parts: to_spawn and to_overwrite
    // put them in StateToRestore
    state_to_restore.to_spawn = snapshot.0.states; std::mem::take(snapshot.states);
    for (ent,&r) in &query {
        if let Some(state) = state_to_restore.to_spawn.remove(&r) {
            state_to_restore.to_overwrite.insert(r,state);
        }else{
            //state_to_restore.to_delete.push(ent);
        }
    }
}

impl Default for RollbackPlugin {
    fn default() -> Self {
        Self {
            rollback_processing_schedule: Some(Update.dyn_clone()), //TODO: this might better be FixedUpdate
            //or at least just for some systems (get local input and run rollback? actually not run rollback as we might need to do rollback after any network input)
        }
    }
}

//TODO: use *_system for names of systems probably?
impl Plugin for RollbackPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(RollbackSchedule);    // user can app.add_schedule(custom_schedule)
        if let Some(schedule) = self.rollback_processing_schedule {
            app.configure_sets(schedule,
                (
                    //(get_input_from_local,get_input_from_network),
                    //(send_input_to_network,run_rollback_schedule),
                    //OR:
                    //get_input_from_local,
                    //network_io,
                    //run_rollback_schedule,
                    //OR:
                    RollbackProcessSet::ProcessIO,  //TODO: the local input should be probably gathered at fixed frame intervals, just like the RollbackSchedule
                    //RollbackProcessSet::SaveCurrentInput, TODO: this should run before the first loop in RollbackSchedule
                    // -> where to put this? it just needs to do this:
                    //save inputs of current frame
                    //snapshots.buffer.back_mut().expect("should contain at least one Snapshot").save_inputs(world);
                    //this can be probably just a method on Snapshot, same as .restore_inputs()
                    RollbackProcessSet::RunRollbackSchedule,
                ).chain()
            )
            .add_systems(schedule,
                (
                    run_rollback_schedule_system.in_set(RollbackProcessSet::RunRollbackSchedule),
                )
            );
        }

        //TODO: use this in systems for optimizations: resource_equals(RollbackFirstLoop(true))
        app.configure_sets(RollbackSchedule,
            (
                RollbackSet::PrepareRestore,
                RollbackSet::RestoreState,
                RollbackSet::Update,
                RollbackSet::SaveState,
            ).chain(),
        )
        .configure_sets(RollbackSchedule,
            (
                RollbackSet::PrepareRestore,
                RollbackSet::RestoreState,
            ).run_if(resource_exists::<SnapshotToRestore>()),
        )
        .add_systems(RollbackSchedule,
            (
                prepare_to_restore.in_set(RollbackSet::PrepareRestore),
                //TODO: should anything go here?
            )
        );
    }
}

pub fn run_rollback_schedule_system<S,I: Clone>(world: &mut World) {
    // 1. save current input
    // 1.1. save inserted future Snapshots if they exist      ?
    //      2. restore and simulate past frames
    //          run the loop
    //      2. prepare new Snapshot for the next frame

    // save current input   TODO: what if it does not exist?
    let inputs = world.resource::<I>().clone();
    let last = world.resource_mut::<Snapshots>().buffer.get_mut(0)
        .expect("should contain at least one Snapshot");
    last.inputs = inputs;
    last.modified = true;   //TODO: is this needed?     probably yes currently

    // restore modified snapshots
    let mut snapshots = world.resource::<Snapshots<S,I>>();
    let snapshots_len = snapshots.buffer.len();
    
    let mut current_snapshot = 0;   //the index

    for i in (0..snapshots_len).rev() {
        let snapshot = snapshots.buffer.get(i).expect("0..snapshots.buffer.len() was used for index");
        if snapshot.modified {
            let inputs = snapshot.inputs.clone();
            
            if current_snapshot!=i {
                let snapshot = snapshot.clone();    //TODO: maybe only use the state?
                world.insert_resource(SnapshotToRestore);
                world.insert_resource(SnapshotToRestore(snapshot));
                current_snapshot = i;
            }

            world.insert_resource(inputs);
            world.insert_resource(Snapshot::default());
            
            world.insert_resource(CurrentSnapshotIndex(current_snapshot));
            world.run_schedule(RollbackSchedule);
            current_snapshot -= 1;      //TODO:

            let state_to_save = world.remove_resource::<Snapshot>()  //TODO: use StateToSave instead
                .expect("this Resource should always exist after running the RollbackSchedule");

            if i!=0 {
                let next_snapshot = snapshots.buffer.get_mut(i-1).expect("i is not 0");
                *next_snapshot = Snapshot {
                    states: state_to_save.states,
                    inputs: next_snapshot.inputs,
                    modified: true,
                };
                next_snapshot.save_state(world);
            }else{
                //TODO: push new Snapshot probably?
                let _ = snapshots.buffer.pop_back();
                snapshots.buffer.push_front(state_to_save);
            }
        }
    }
    //I probably need to restore the last Snapshot here (at least the Inputs)




    //the old code:
    /*
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH).expect("since UNIX_EPOCH")
        .as_millis();
    let mut future = world.remove_resource::<FuturePastSnapshot<S>>();
    
    world.resource_scope(|world, mut snapshots: Mut<Snapshots<S>>| {
        let _target_frame = future.as_ref().map(|x| x.frame).unwrap_or(snapshots.frame)+1;
        while /*snapshots.frame<_target_frame ||*/ snapshots.last_frame_time<now {
            //println!("rollback update now {now} target time {} frame {} target frame {}",snapshots.last_frame_time,snapshots.frame,_target_frame);
            //insert the future snapshot when the correct frame is reached
            if let Some(f) = future.as_ref() {
                if f.frame==snapshots.frame {
                    let snapshot = future.take().expect("future contains value").snapshot;
                    if !snapshot.states.is_empty() {
                        snapshot.restore_state(world);
                    }
                    if !snapshot.inputs.0.is_empty() {
                        snapshot.restore_inputs(world);
                    }
                }
            }

            //save inputs of current frame
            snapshots.buffer.back_mut().expect("should contain at least one Snapshot").save_inputs(world);
            //prepare new empty snapshot - the next frame
            snapshots.buffer.push_back(Snapshot::default());
            snapshots.frame += 1;
            snapshots.last_frame_time += 1000/60; //TODO: move into constant

            let mut needs_restore = false;  //TODO: optimize -> instead store last loaded snapshot and do not restore when it is already loaded
            let len  = snapshots.buffer.len();
            for i in 0..len-2 {
                let snapshot = snapshots.buffer.get_mut(i)
                    .expect("index i is always < length-2");
                
                if snapshot.modified {
                    //println!("rollback modified index {i}");
                    snapshot.modified = false;
                    needs_restore = true;

                    snapshot.restore(world);    //TODO: this does not need to restore the state if it was saved in the previous iteration
                    world.run_schedule(RollbackSchedule);
                    let next_snapshot = snapshots.buffer.get_mut(i+1)
                        .expect("index i is always < length-2");
                    next_snapshot.modified = true;
                    next_snapshot.save_state(world);
                }
            }

            let snapshot = snapshots.buffer.get_mut(len-2)
                .expect("second last snapshot should exist");

            //println!("last frame index {} modified {} needs restore {needs_restore}",len-2,snapshot.modified);
            snapshot.modified = false;
            if needs_restore {  //TODO: can this be instead checked by snapshot.modified?
                snapshot.restore(world);
            }
            world.run_schedule(RollbackSchedule);
            let next_snapshot = snapshots.buffer.back_mut()
                .expect("should contain at least one Snapshot");
            next_snapshot.modified = false;
            next_snapshot.save_state(world);
        }
    });
    if let Some(future) = future.take() {
        world.insert_resource(future);
    }
    */
}

//TODO: rollback for resources