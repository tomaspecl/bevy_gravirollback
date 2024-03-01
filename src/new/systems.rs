use super::*;

use bevy::prelude::*;

//when the entity is despawned, its snapshot storage will be moved into DeletedStorage
//and when it is needed to restore this deleted entity it will be used. When the frames of
//a deleted entity are completely empty (all the frames have the entity removed), it will be deleted from DeletedStorage.
//this will use RollbackDespawnMarker

//if going back in time and a certain entity does not exist at that time, it can be safely deleted completely
//as it will be respawned by the same method it was created before the time shift

//whenever a rollback entity is created the creation procedure should be saved (perhaps as an Input)
//such that it can be restored at any time when needed.
//this will use RollbackSpawnMarker(spawn_system_callback)


// | state + process IO -> execute -> | state + process IO -> execute -> | state   ...
// | get spawn command -> spawn entity with RollbackSpawnMarker | 

//only despawn entities with Rollback<Exists> having all false, thus indicating that the entity should be removed
pub fn despawn_nonexistent(
    mut commands: Commands,
    query: Query<(Entity, &Rollback<Exists>)>,
) {
    for (e, r) in &query {
        //TODO: use some form of caching, like NonExistentFor(frames: usize)
        //the cached value can be updated when a custom save<Exists> runs
        if r.0.iter().all(|x| !x.0) {
            commands.entity(e).despawn_recursive();
        }
    }
}

//automatically update the RollbackMap when a new RollbackID component is added or removed
//should also work when a rollback entity gets despawned
pub fn update_rollback_map(
    mut map: ResMut<RollbackMap>,
    additions: Query<(Entity, &RollbackID), Added<RollbackID>>,
    mut removals: RemovedComponents<RollbackID>,
) {
    for (e,&r) in &additions {
        if map.0.insert(r, e).is_some() || map.1.insert(e, r).is_some() {
            panic!("Can not add rollback mapping for Entity {e:?} RollbackID {r:?} because it already existed");
        }
    }
    for e in removals.read() {
        if let Some(r) = map.1.remove(&e) {
            if map.0.remove(&r).is_none() {
                panic!("Entity {e:?} did not have a mapping from RollbackID {r:?}");
            }
        }else{
            panic!("Entity {e:?} did not have a mapping to RollbackID");
        }
    }
}

//when the state should be fixed, the user could use a Query parameter to narrow down the Query
// Query<(something),user_supplied_filter>
//fn save<T: Component, Filter: bevy::ecs::query::ReadOnlyWorldQuery = ()>(mut q: Query<(Entity, &Rollback<T>, &mut T), Filter>) {
//    //TODO:
//}

//removes all entities that should not exist in the frame which is being restored, they do not need to be saved
//when an entity is restored then all future existence should be taken as false, and the entity removed
pub fn restore_exists_remove_nonexistent<QueryFilter: ReadOnlyWorldQuery>(
    info: Res<SnapshotInfo>,
    mut query: Query<(Entity, &mut Exists, &Rollback<Exists>), QueryFilter>,    //TODO: use Rollback<Exists> instead of Rollback<Option<Exists>>
    mut commands: Commands,
) {
    let first = info.last.saturating_sub(SNAPSHOTS_LEN as u64 - 1);
    'outer: for (e, mut existence, r) in &mut query {
        let ex = r.0[info.current_index()];
        *existence = ex;
        if !ex.0 {
            for i in (first..info.current).map(|frame| info.index(frame)) {
                if r.0[i].0 {
                    continue 'outer;    //the entity exists
                }
            }
            commands.entity(e).despawn_recursive(); //the entity does not exist, despawn it
        }
    }
}

//TODO: allow using Bundles, tuples, etc... for T, example: Rollback<(Transform, Velocity)>
//the default restore and save rollback systems, the user can use their own
pub fn restore<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(T::RestoreQuery<'_>, &Rollback<T>), With<RollbackID>>,
) {
    restore_filter(info, query);
}

pub fn restore_option<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(Entity, Option<T::RestoreQuery<'_>>, &Rollback<Option<T>>), With<RollbackID>>,
    commands: Commands,
) {
    restore_option_filter(info, query, commands);
}

pub fn save<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(T::SaveQuery<'_>, &mut Rollback<T>), With<RollbackID>>,
) {
    save_filter(info, query);
}

pub fn save_option<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(Option<T::SaveQuery<'_>>, &mut Rollback<Option<T>>), With<RollbackID>>,
) {
    save_option_filter(info, query);
}

pub fn restore_filter<T: RollbackCapable, QueryFilter: ReadOnlyWorldQuery>(
    info: Res<SnapshotInfo>,
    mut query: Query<(T::RestoreQuery<'_>, &Rollback<T>), QueryFilter>,
) {
    for (q, r) in &mut query {
        r.0[info.current_index()].restore(q);
    }
}

pub fn restore_option_filter<T: RollbackCapable, QueryFilter: ReadOnlyWorldQuery>(
    info: Res<SnapshotInfo>,
    mut query: Query<(Entity, Option<T::RestoreQuery<'_>>, &Rollback<Option<T>>), QueryFilter>,
    mut commands: Commands,
) {
    for (e, q, r) in &mut query {
        match (&r.0[info.current_index()], q) {
            (Some(to_restore), None) => to_restore.insert(e, &mut commands),
            (Some(to_restore), Some(q)) => to_restore.restore(q),
            (None, Some(_)) => T::remove(e, &mut commands),
            (None, None) => (),
        }
    }
}

pub fn save_filter<T: RollbackCapable, QueryFilter: ReadOnlyWorldQuery>(
    info: Res<SnapshotInfo>,
    mut query: Query<(T::SaveQuery<'_>, &mut Rollback<T>), QueryFilter>,
) {
    for (q, mut r) in &mut query {
        r.0[info.current_index()] = T::save(q);
    }
}

pub fn save_option_filter<T: RollbackCapable, QueryFilter: ReadOnlyWorldQuery>(
    info: Res<SnapshotInfo>,
    mut query: Query<(Option<T::SaveQuery<'_>>, &mut Rollback<Option<T>>), QueryFilter>,
) {
    for (q, mut r) in &mut query {
        r.0[info.current_index()] = q.map(T::save);
    }
}

//make the same systems for Resources
//the user can then use arbitrary types for storing Inputs and still have rollback work for them

pub fn restore_resource<T: Resource + Clone + Default>( //TODO: remove Default requirement
    info: Res<SnapshotInfo>,
    rollback: Res<Rollback<T>>,
    mut resource: ResMut<T>,
) {
    *resource = rollback.0[info.current_index()].clone();
}

pub fn restore_resource_option<T: Resource + Clone>(
    info: Res<SnapshotInfo>,
    rollback: Res<Rollback<Option<T>>>,
    resource: Option<ResMut<T>>,
    mut commands: Commands,
) {
    if let Some(res) = &rollback.0[info.current_index()] {
        if let Some(mut resource) = resource {
            *resource = res.clone();
        }else{
            commands.insert_resource(res.clone());
        }
    }else{
        commands.remove_resource::<T>();
    }
}

pub fn save_resource<T: Resource + Clone + Default>( //TODO: remove Default requirement
    info: Res<SnapshotInfo>,
    mut rollback: ResMut<Rollback<T>>,
    resource: Res<T>,
) {
    rollback.0[info.current_index()] = resource.clone();
}

pub fn save_resource_option<T: Resource + Clone>(
    info: Res<SnapshotInfo>,
    mut rollback: ResMut<Rollback<Option<T>>>,
    resource: Option<Res<T>>,
) {
    rollback.0[info.current_index()] = resource.map(|x| x.clone());
}

//more like clear_input, save_input should run after RollbackProcessSet::HandleIO
//this should not be needed to run in RollbackSchedule because this will delete previously collected inputs
pub fn save_resource_input_option<T: Resource + Clone>(
    info: Res<SnapshotInfo>,
    mut rollback: ResMut<Rollback<Option<T>>>,
) {
    rollback.0[info.current_index()] = None;
}

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
        if time.elapsed() > std::time::Duration::from_millis(1000/60) {break 'lop}  //TODO: the time here is arbitrary

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
            print!("*");
            let next_index = (index+1)%SNAPSHOTS_LEN;
            let next_frame = frame+1;

            snapshots[next_index].frame = next_frame;

            if next_frame>last {
                snapshots[next_index].modified = false;
                info.last = next_frame;
                
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
}

//TODO: default systems that will take SnapshotUpdateEvent<T> that will simplify the usage

//TODO: default systems that will create some default interface that will simplify communication between client and server
// some reasonable defaults, refactor as much common work as possible out into this default system
// these systems could define a simple interface and basically tell the user what he should send across the connection
// and tell him what to do with anything that is received -> maybe do it through Events?