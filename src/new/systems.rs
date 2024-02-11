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

pub fn respawn(world: &mut World) {
    let index = world.resource::<SnapshotToRestore>().index;   //TODO: I should not need to check anything here, DespawnedRollbackEntities should be pruned already
    let mut to_spawn = Vec::new();
    world.resource_mut::<DespawnedRollbackEntities>().entities.retain_mut(|e| {
        if e.existence.0[index].is_some() {
            to_spawn.push(RollbackEntityEntry {
                id: e.id,
                spawn_system: std::mem::take(&mut e.spawn_system),
                existence: std::mem::take(&mut e.existence),
                rollback_data: std::mem::take(&mut e.rollback_data),
            });
            false
        }else{
            true
        }
    });

    world.resource_scope(|world, mut map: Mut<RollbackMap>| {
        for e in to_spawn {
            if e.existence.0[index].is_some() {
                let mut system = e.spawn_system.expect("Rollback Entity marked for Despawning needs a RollbackSpawnMarker").0;
                //system.initialize(world);   //TODO: maybe check if it was already initialized? -> maybe check how world.run_schedule() does it
                let entity = system.run((), world);     //TODO: why does it need mutability of the system?
                system.apply_deferred(world); //TODO: is this needed? if yes then I need to start putting this in more places
                let _ = map.0.insert(e.id, entity);     //TODO: maybe should panic when key existed
                let mut entity_mut = world.entity_mut(entity);
                entity_mut.insert(RollbackSpawnMarker(system));
                
                for data in e.rollback_data {
                    data.restore(&mut entity_mut);
                }
            }
        }
    });
}

pub fn despawn(world: &mut World) {
    let entities = world.query_filtered::<Entity, With<RollbackDespawnMarker>>().iter(world).collect::<Vec<_>>();

    world.resource_scope(|world2, mut despawned: Mut<DespawnedRollbackEntities>|
        world2.resource_scope(|world, registry: Mut<RollbackRegistry>| {
            for entity in entities {
                let mut e = world.entity_mut(entity);
                let id = e.take::<RollbackID>().expect("Rollback Entity marked for Despawning needs a RollbackId");
                let spawn_system = e.take::<RollbackSpawnMarker>().expect("Rollback Entity marked for Despawning needs a RollbackSpawnMarker");
                let existence = e.take::<Rollback<Existence>>().expect("Rollback Entity marked for Despawning needs a Rollback<Existence>");
                let rollback_data = registry.getters.iter().filter_map(|f| f(&mut e)).collect();
        
                despawned.entities.push(RollbackEntityEntry {
                    id,
                    spawn_system: Some(spawn_system),
                    existence,
                    rollback_data,
                });
        
                e.despawn_recursive();
            }
    }));
}

//when the state should be fixed, the user could use a Query parameter to narrow down the Query
// Query<(something),user_supplied_filter>
//fn save<T: Component, Filter: bevy::ecs::query::ReadOnlyWorldQuery = ()>(mut q: Query<(Entity, &Rollback<T>, &mut T), Filter>) {
//    //TODO:
//}

//removes all entities that should not exist in the frame which is being restored, they do not need to be saved
//this should run before other restore<T>
pub fn restore_remove_nonexistent(
    index: Res<SnapshotToRestore>,
    query: Query<(Entity, &Rollback<Existence>), With<RollbackID>>,    //TODO: is With<RollbackId> needed?
    mut commands: Commands,
) {
    for (e, existance) in &query {
        if existance.0[index.index].is_none() {
            commands.entity(e).despawn_recursive();
        }
    }
}

//the default restore and save rollback systems, the user can use their own
pub fn restore<T: Component + Clone>(
    index: Res<SnapshotToRestore>,
    mut query: Query<(Entity, Option<&mut T>,&Rollback<T>), With<RollbackID>>,    //TODO: is With<RollbackId> needed?
    mut commands: Commands,
) {
    for (e, c, r) in &mut query {
        if let Some(component) = &r.0[index.index] {
            if let Some(mut c) = c {
                *c = component.clone();
            }else{
                commands.entity(e).insert(component.clone());
            }
        }else{
            commands.entity(e).remove::<T>();
        }
    }
}

pub fn save<T: Component + Clone>(
    index: Res<SnapshotToSave>,
    mut query: Query<(Option<&T>,&mut Rollback<T>), With<RollbackID>>,    //TODO: is With<RollbackId> needed?
) {
    for (c, mut r) in &mut query {
        r.0[index.index] = c.map(|x| x.clone());
    }
}

pub fn save_existence(
    index: Res<SnapshotToSave>,
    mut query: Query<(Has<RollbackDespawnMarker>,&mut Rollback<Existence>), With<RollbackID>>,    //TODO: is With<RollbackId> needed?
) {
    for (despawn, mut r) in &mut query {
        r.0[index.index] = if despawn {None}else{Some(Existence)};
    }
}

//TODO: automatically update the RollbackMap when a new RollbackId component is added
pub fn update_rollback_map(
    mut map: ResMut<RollbackMap>,
    query: Query<(Entity, &RollbackID), Added<RollbackID>>,
) {
    for (e,&r) in &query {
        map.0.insert(r, e);
    }
}

//make the same systems for Resources
//the user can then use arbitrary types for storing Inputs and still have rollback work for them

pub fn restore_resource<T: Resource + Clone>(
    index: Res<SnapshotToRestore>,
    rollback: Res<Rollback<T>>,
    resource: Option<ResMut<T>>,
    mut commands: Commands,
) {
    if let Some(res) = &rollback.0[index.index] {
        if let Some(mut resource) = resource {
            *resource = res.clone();
        }else{
            commands.insert_resource(res.clone());
        }
    }else{
        commands.remove_resource::<T>();
    }
}

pub fn save_resource<T: Resource + Clone>(
    index: Res<SnapshotToSave>,
    mut rollback: ResMut<Rollback<T>>,
    resource: Option<ResMut<T>>,
) {
    rollback.0[index.index] = resource.map(|x| x.clone());
}

//more like clear_input, save_input should run after RollbackProcessSet::HandleIO
//this should not be needed to run in RollbackSchedule because this will delete previously collected inputs
pub fn save_resource_input<T: Resource + Clone>(
    index: Res<SnapshotToSave>,
    mut rollback: ResMut<Rollback<T>>,
) {
    rollback.0[index.index] = None;
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

'lop: loop {
    if time.elapsed() > std::time::Duration::from_millis(1000/60) {break 'lop}

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
        
        let input_only = frame == info.current;
        //println!("run_rollback_schedule_system restoring frame {frame} index {index} input_only {input_only}");
        world.insert_resource(SnapshotToRestore {
            index,
            frame,
            input_only,
        });

        world.insert_resource(SnapshotToSave {
            index: next_index,
            frame: next_frame,
        });

        world.run_schedule(RollbackSchedule);

        world.resource_mut::<SnapshotInfo>().current = next_frame;
    }else{
        break 'lop
    }
    break 'lop  //disable the loop for now, it is not needed thanks to window.present_mode = PresentMode::AutoNoVsync; which makes the Update schedule update as fast as possible
}
}

//TODO: default systems that will take SnapshotUpdateEvent<T> that will simplify the usage

//TODO: default systems that will create some default interface that will simplify communication between client and server
// some reasonable defaults, refactor as much common work as possible out into this default system
// these systems could define a simple interface and basically tell the user what he should send across the connection
// and tell him what to do with anything that is received -> maybe do it through Events?