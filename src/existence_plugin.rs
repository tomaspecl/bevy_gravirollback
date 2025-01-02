use bevy::ecs::query::QueryFilter;

use crate::*;
use crate::schedule_plugin::*;

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





//IDEA:
//instead of having RollbackSpawnMarker with a system, have Event rollback functioning and
//instead of RespawnRemove system set have Spawn system set in which spawn systems will be placed 
//those spawn systems will accept SpawnEvent and create the needed entity
//this will be used for spawning all entities and respawning them during rollback, it will be the same process

pub struct ExistencePlugin;

impl Plugin for ExistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(RollbackSchedule,(
            restore_exists_remove_nonexistent::<systems::DefaultFilter>
                .in_set(RollbackSet::Restore),
            systems::save::<Exists>
                .in_set(RollbackSet::Save),
            despawn_nonexistent
                .in_set(RollbackSet::Despawn),
        ));
    }
}

//can use Query<..., Changed<Exists>> to run code that handles the "virtual" despawn and respawn when needed
#[derive(Component, Reflect, Clone, Copy)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct Exists(pub bool);
impl Default for Exists {
    fn default() -> Self { Exists(false) }  //TODO: maybe it should be Exists(true)
}




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

//removes all entities that should not exist in the frame which is being restored, they do not need to be saved
//when an entity is restored then all future existence should be taken as false, and the entity removed
pub fn restore_exists_remove_nonexistent<Filter: QueryFilter>(
    info: Res<SnapshotInfo>,
    mut query: Query<(Entity, &mut Exists, &Rollback<Exists>), Filter>,
    mut commands: Commands,
) {
    let first = info.last.saturating_sub(SNAPSHOTS_LEN as u64 - 1);
    'outer: for (e, mut existence, r) in &mut query {
        let ex = r.0[info.current_index()];
        *existence = ex;
        if !ex.0 {
            println!("checking despawning entity {e:?}");
            for i in (first..info.current).map(|frame| info.index(frame)) {
                if r.0[i].0 {
                    continue 'outer;    //the entity exists
                }
            }
            println!("despawning entity {e:?}");
            commands.entity(e).despawn_recursive(); //the entity does not exist, despawn it
        }
    }
}