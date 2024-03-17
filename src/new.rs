pub mod systems;
pub mod for_user;

use bevy::prelude::*;
use bevy::ecs::schedule::{ScheduleLabel, SystemConfigs};
use bevy::ecs::query::{ReadOnlyWorldQuery, WorldQuery};
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};
use bevy::utils::HashMap;

// *****************************
//TODO: check comments and names and update them
// *****************************

// A snapshot contains the state of rollback entities and the player inputs of a single game frame

// The rollback schedule gets new player inputs and combines them with the current state by running the Update.
// The Update generates a new state which will be saved.
// If some old saved state or input gets updated then the rollback schedule will load that past snapshot
// and rerun it up to the present state.


//TODO: ideas that could make it better
//bevy::ecs::system::SystemState can be used for caching access to certain data through &mut World -> speed up
//let components = world.inspect_entity(entity);     //can be used to get Components of an entity

//TODO: component change detection should work correctly even when changing frames -> maybe use marker Component to signal change?

//TODO: figure out how to remove the Default requirement for #[reflect(Resource)]

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
        Save if SaveStates,
        DespawnNonExistent,
    }
    //after running the RollbackSchedule loop it should hold that current frame == last frame
}
*/

pub struct RollbackPlugin {
    /// The [`Schedule`] in which rollback processing [`SystemSet`]s will be configured
    pub rollback_processing_schedule: Option<bevy::utils::intern::Interned<dyn ScheduleLabel>>, //TODO: is Interned needed?
}
impl Default for RollbackPlugin {
    fn default() -> Self {
        Self {
            rollback_processing_schedule: Some(Update.intern()), //TODO: this might better be FixedUpdate
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
                    RollbackProcessSet::HandleIO,  //TODO: the local input should be probably gathered at fixed frame intervals, just like the RollbackSchedule
                    RollbackProcessSet::RunRollbackSchedule,
                ).chain()
            )
            .add_systems(schedule,(
                systems::run_rollback_schedule_system.in_set(RollbackProcessSet::RunRollbackSchedule),

                apply_deferred.after(RollbackProcessSet::HandleIO).before(RollbackProcessSet::RunRollbackSchedule),
            ));
        }

        app.configure_sets(RollbackSchedule,(
            (
                (RollbackSet::Restore, RollbackSet::RestoreInputs),
                RollbackSet::Update,
                RollbackSet::Save,
                RollbackSet::Despawn,   //maybe Despawn can run at the same time as Save?
            ).chain(),
            RollbackSet::Restore.run_if(resource_exists::<RestoreStates>()),
            RollbackSet::RestoreInputs.run_if(resource_exists::<RestoreInputs>()),
        ))
        .add_systems(RollbackSchedule,(
            //RollbackSet::Restore
            (
                systems::restore_exists_remove_nonexistent::<With<RollbackID>>,
                //systems::update_rollback_map,
            ).chain().in_set(RollbackSet::Restore),

            apply_deferred.after(RollbackSet::Restore).after(RollbackSet::RestoreInputs).before(RollbackSet::Update),
            
            //RollbackSet::Update

            (
                apply_deferred,
                |mut info: ResMut<SnapshotInfo>| info.current += 1,
            ).after(RollbackSet::Update).before(RollbackSet::Save),

            //RollbackSet::Save
            systems::save::<Exists>.in_set(RollbackSet::Save),

            apply_deferred.after(RollbackSet::Save).before(RollbackSet::Despawn),

            //RollbackSet::Despawn
            systems::despawn_nonexistent.in_set(RollbackSet::Despawn),
            (
                apply_deferred,
                systems::update_rollback_map,
            ).chain().after(RollbackSet::Despawn)
        ));

        app.insert_resource(SnapshotInfo {
            last: 0,
            current: 0,
            snapshots: vec![Snapshot { frame: 0, modified: false };SNAPSHOTS_LEN],
        })
        .insert_resource(RollbackMap(HashMap::new(), HashMap::new()))
        ;
    }
}

#[derive(Component, Reflect, Clone, Copy, Hash, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct RollbackID(pub u64);

pub const SNAPSHOTS_LEN: usize = 128;    //TODO: how to make this changable by the user of this library? maybe extern?
//maybe I can wrap the whole lib in a macro and let the user instanciate it with their specified SNAPSHOTS_LEN? this would be crazy
//maybe some configuration/compilation flag can be passed in Cargo.toml ?

//TODO: use a flag to choose if the size is growable, size is fixed by default
#[derive(Component, Resource, Reflect, Clone)]
#[reflect(Resource)]
pub struct Rollback<T: Default /* ideally remove this bound, it is mainly for Reflect */>(pub [T; SNAPSHOTS_LEN]);   //this version has fixed size, it should be faster as there is no pointer dereference, question is if it matters or was it just premature optimization
impl<T: Default> Default for Rollback<T> {
    fn default() -> Self {
        Self(std::array::from_fn(|_| T::default()))     //this had to be done manualy because the #[derive(Default)] macro could not handle it with big SNAPSHOTS_LEN
    }
}

// use std::collections::VecDeque;
// struct Rollback<T>(VecDeque<T>);  //this version is dynamicaly growable during runtime

//TODO: split this enum into two enums, one will be pub and second not pub
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum RollbackSet {
    /// Restore the state of rollback entities that are needed to be restored. Restore Resources.
    Restore,
    /// Restore inputs
    RestoreInputs,
    /// Run the game update step
    Update,
    /// Save the state of rollback entities and Resources (not inputs)
    Save,
    /// Despawn rollback entities that use Rollback<Exists> and have no frames with existence
    Despawn,
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
/// This [`SystemSet`] specifies the high level rollback steps. Those are:
/// 1. Getting all the [`Inputs`] (current or old/delayed) from all "players"
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

pub trait RollbackCapable: Default + Send + Sync + 'static {    //TODO: remove Default requirement
    type RestoreQuery<'a>: WorldQuery;
    /// Extra restore system parameters that can be used for anything
    type RestoreExtraParam: SystemParam;
    type SaveQuery<'a>: WorldQuery;
    // Extra save system parameters that can be used for anything
    type SaveExtraParam: SystemParam;
    fn restore(&self, q: <Self::RestoreQuery<'_> as WorldQuery>::Item<'_>, extra: &mut StaticSystemParam<Self::RestoreExtraParam>);
    fn save(q: <Self::SaveQuery<'_> as WorldQuery>::Item<'_>, extra: &mut StaticSystemParam<Self::SaveExtraParam>) -> Self;

    //for inserting and removing components and other init
    //this can be used for spawning rollback entities and respawning them, and also sending them across network
    //for example struct PlayerMarker; will have
    //fn insert(&self, commands: Commands) spawn all the other components that are not rollback
    fn insert(&self, _entity: Entity, _commands: &mut Commands) {unimplemented!()}
    fn remove(_entity: Entity, _commands: &mut Commands) {unimplemented!()}
}

impl<T: Component + Clone + Default> RollbackCapable for T  //TODO: remove Default requirement
//where
//    T: NotTupleHack
{
    type RestoreQuery<'a> = &'a mut T;
    type RestoreExtraParam = ();
    type SaveQuery<'a> = &'a T;
    type SaveExtraParam = ();

    fn restore(&self, mut q: Mut<T>, _extra: &mut StaticSystemParam<()>) {
        *q = self.clone();
    }

    fn save(q: &T, _extra: &mut StaticSystemParam<()>) -> Self {
        q.clone()
    }

    fn insert(&self, entity: Entity, commands: &mut Commands) {
        commands.entity(entity).insert(self.clone());
    }

    fn remove(entity: Entity, commands: &mut Commands) {
        commands.entity(entity).remove::<T>();
    }
}

/*
//negative impls
//https://github.com/rust-lang/rust/issues/68318
trait NotTupleHack {}
impl<T> NotTupleHack for T {}
macro_rules! tuple_hack_impl {
    ($($T: ident),*) => { impl<$($T,)*> !NotTupleHack for ($($T,)*) {} }
}
macro_rules! tuple_impl {
    ($(($T: ident, $t: ident, $s: ident)),*) => {
        impl<$($T: Component + Clone),*> RollbackCapable3 for ($($T,)*) {
            type QueryItem = ();
            type RestoreQuery<'a> = ($(&'a mut $T,)*);

            fn restore(&self, ($(mut $t,)*): ($(Mut<'_, $T>,)*)) {
                let ($($s,)*) = self;
                $(*$t = $s.clone();)*
            }

            fn insert(&self, entity: Entity, mut commands: Commands) {
                commands.entity(entity).insert(self.clone());
            }
        
            fn remove(entity: Entity, mut commands: Commands) {
                commands.entity(entity).remove::<Self>();
            }
        }
    }
}
use bevy_utils::all_tuples;
all_tuples!(tuple_hack_impl, 1, 15, T);
all_tuples!(tuple_impl, 1, 15, T, t, s);
*/

//so that the user can pick Rollback<Optiuon<T>> or Rollback<T>
/*impl<T: RollbackCapable3> RollbackCapable3 for Option<T> {
    type QueryItem = Option<T>;
    type RestoreQuery<'a> = Option<T::RestoreQuery<'a>>;
}*/

//then user can do:
//register_rollback::<T>
//or
//register_rollback::<Option<T>>

pub trait RollbackSystems {
    fn get_default_rollback_systems() -> SystemConfigs {
        Self::get_default_rollback_systems_filtered::<With<RollbackID>>()
    }
    fn get_default_rollback_systems_option() -> SystemConfigs {
        Self::get_default_rollback_systems_option_filtered::<With<RollbackID>>()
    }
    fn get_default_rollback_systems_filtered<QueryFilter: ReadOnlyWorldQuery + 'static>() -> SystemConfigs;
    fn get_default_rollback_systems_option_filtered<QueryFilter: ReadOnlyWorldQuery + 'static>() -> SystemConfigs;
}

impl<T: RollbackCapable> RollbackSystems for T {
    fn get_default_rollback_systems_filtered<QueryFilter: ReadOnlyWorldQuery + 'static>() -> SystemConfigs {
        (
            systems::restore_filter::<T, QueryFilter>.in_set(RollbackSet::Restore),
            systems::save_filter::<T, QueryFilter>.in_set(RollbackSet::Save),
        ).into_configs()
    }
    fn get_default_rollback_systems_option_filtered<QueryFilter: ReadOnlyWorldQuery + 'static>() -> SystemConfigs {
        (
            systems::restore_option_filter::<T, QueryFilter>.in_set(RollbackSet::Restore),
            systems::save_option_filter::<T, QueryFilter>.in_set(RollbackSet::Save),
        ).into_configs()
    }
}

pub trait RollbackStorage: Send + Sync {
    fn restore(self: Box<Self>, entity_mut: &mut EntityWorldMut);  //TODO: can this be moved to RollbackRegistry?
}
impl<T: Default + 'static + Clone + Send + Sync> RollbackStorage for Rollback<T> {
    fn restore(self: Box<Self>, entity_mut: &mut EntityWorldMut) {
        entity_mut.insert(*self);
    }
}

#[derive(Resource)]
pub struct RollbackRegistry {
    pub getters: Vec<Getter>,  //TODO: maybe it should be a HashMap<ComponentId, fn(...)->...> or similar for optimization
}

pub type Getter = fn(&mut EntityWorldMut) -> Option<Box<dyn RollbackStorage>>;

//IDEA:
//instead of having RollbackSpawnMarker with a system, have Event rollback functioning and
//instead of RespawnRemove system set have Spawn system set in which spawn systems will be placed 
//those spawn systems will accept SpawnEvent and create the needed entity
//this will be used for spawning all entities and respawning them during rollback, it will be the same process

//can use Query<..., Changed<Exists>> to run code that handles the "virtual" despawn and respawn when needed
#[derive(Component, Reflect, Clone, Copy)]
pub struct Exists(pub bool);
impl Default for Exists {
    fn default() -> Self { Exists(false) }  //TODO: maybe it should be Exists(true)
}

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct RollbackMap(pub HashMap<RollbackID, Entity>, pub HashMap<Entity, RollbackID>);   //TODO: this should be generic over RollbackID
impl RollbackMap {
    pub fn remove(&mut self, e: Entity) {
        if let Some(r) = self.1.get(&e) {
            if let Some(e2) = self.0.get(r) {
                if *e2!=e {
                    panic!("Entity {e:?} RollbackID {r:?} had mapping to Entity {e2:?}");
                }
                self.0.remove(r);
                self.1.remove(&e);
            }else{
                panic!("Entity {e:?} did not have a mapping from RollbackID {r:?}");
            }
        }else{
            warn!("Entity {e:?} did not have a mapping to RollbackID");
        }
    }
    pub fn insert(&mut self, e: Entity, r: RollbackID) {
        match (self.0.get(&r), self.1.get(&e)) {
            (None, None) => {
                self.0.insert(r, e);
                self.1.insert(e, r);
            },
            (Some(e2), Some(r2)) => {
                if *e2!=e || *r2!=r {
                    panic!("Can not add rollback mapping for Entity {e:?} RollbackID {r:?} because {e2:?}, {r2:?} already existed");
                }
            },
            (Some(e2), None) => panic!("RollbackMap was incomplete Entity {e2:?} (insert with {e:?}, {r:?})"),
            (None, Some(r2)) => panic!("RollbackMap was incomplete RollbackID {r2:?} (insert with {e:?}, {r:?})"),
        }
    }
}

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

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct SnapshotInfo {
    /// The last frame in storage
    pub last: u64,
    /// The currently loaded frame, or the frame that should be restored
    pub current: u64,
    pub snapshots: Vec<Snapshot>,   //TODO: this should probably not belong to here
}
impl SnapshotInfo {
    /// Compute the index in storage of the specified frame
    pub fn index(&self, frame: u64) -> usize { (frame%SNAPSHOTS_LEN as u64) as usize }  //TODO: &self is not needed now but in the future SNAPSHOTS_LEN could be part of the struct
    /// Compute the index in storage of the current frame
    pub fn current_index(&self) -> usize { self.index(self.current) }
}

#[derive(Reflect, Clone)]
pub struct Snapshot {
    pub frame: u64,
    pub modified: bool,
}