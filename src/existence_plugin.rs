use bevy::ecs::query::QueryFilter;

use crate::*;
use crate::schedule_plugin::*;

//whenever a rollback entity is created the creation procedure should be saved (perhaps as an Input)
//such that it can be restored at any time when needed.

pub struct ExistencePlugin<const LEN: usize>;

impl<const LEN: usize> Plugin for ExistencePlugin<LEN> {
    fn build(&self, app: &mut App) {
        app
        .add_systems(RollbackRestore, restore_exists_remove_nonexistent::<LEN, systems::DefaultFilter>)
        .add_systems(RollbackSave, (
            systems::save::<Exists, LEN>,
            despawn_nonexistent::<LEN>,
        ).chain());
    }
}

//can use Query<..., Changed<Exists>> to run code that handles the "virtual" despawn and respawn when needed
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct Exists(pub bool);
impl Default for Exists {
    fn default() -> Self { Exists(false) }  //TODO: maybe it should be Exists(true)
}

//only despawn entities with Rollback<Exists> having all false, thus indicating that the entity should be removed
pub fn despawn_nonexistent<const LEN: usize>(
    mut commands: Commands,
    query: Query<(Entity, &Rollback<Exists, LEN>)>,
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
//as it will be respawned by the same method it was created before the time shift
pub fn restore_exists_remove_nonexistent<const LEN: usize, Filter: QueryFilter>(
    current_frame: Res<Frame>,
    last_frame: Res<LastFrame>,
    mut query: Query<(Entity, &mut Exists, &Rollback<Exists, LEN>), Filter>,
    mut commands: Commands,
) {
    let oldest_frame = last_frame.0.saturating_sub(LEN as u64 - 1);
    let current_index = crate::index::<LEN>(current_frame.0);

    'outer: for (e, mut existence, r) in &mut query {
        let ex = r.0[current_index];
        *existence = ex;
        if !ex.0 {
            println!("checking despawning entity {e:?}");
            for i in (oldest_frame..current_frame.0).map(|frame| crate::index::<LEN>(frame)) {
                if r.0[i].0 {
                    continue 'outer;    //the entity exists
                }
            }
            println!("despawning entity {e:?}");
            commands.entity(e).despawn_recursive(); //the entity does not exist, despawn it
        }
    }
}