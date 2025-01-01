use bevy::prelude::*;
use bevy::ecs::query::{WorldQuery, QueryData, QueryFilter};
use bevy::ecs::schedule::SystemConfigs;
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};

use crate::*;
use crate::schedule_plugin::*;

pub trait RollbackCapable: Default + Send + Sync + 'static {    //TODO: remove Default requirement
    type RestoreQuery<'a>: QueryData;
    /// Extra restore system parameters that can be used for anything
    type RestoreExtraParam<'a>: SystemParam;
    type SaveQuery<'a>: QueryData;
    /// Extra save system parameters that can be used for anything
    type SaveExtraParam<'a>: SystemParam;
    fn restore(&self, q: <Self::RestoreQuery<'_> as WorldQuery>::Item<'_>, extra: &mut StaticSystemParam<Self::RestoreExtraParam<'_>>);
    fn save(q: <Self::SaveQuery<'_> as WorldQuery>::Item<'_>, extra: &mut StaticSystemParam<Self::SaveExtraParam<'_>>) -> Self;

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
    type RestoreExtraParam<'a> = ();
    type SaveQuery<'a> = &'a T;
    type SaveExtraParam<'a> = ();

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
    fn get_default_rollback_systems_filtered<F: QueryFilter + 'static>() -> SystemConfigs;
    fn get_default_rollback_systems_option_filtered<F: QueryFilter + 'static>() -> SystemConfigs;
}

impl<T: RollbackCapable> RollbackSystems for T {
    fn get_default_rollback_systems_filtered<F: QueryFilter + 'static>() -> SystemConfigs {
        (
            systems::restore_filter::<T, F>.in_set(RollbackSet::Restore),
            systems::save_filter::<T, F>.in_set(RollbackSet::Save),
        ).into_configs()
    }
    fn get_default_rollback_systems_option_filtered<F: QueryFilter + 'static>() -> SystemConfigs {
        (
            systems::restore_option_filter::<T, F>.in_set(RollbackSet::Restore),
            systems::save_option_filter::<T, F>.in_set(RollbackSet::Save),
        ).into_configs()
    }
}





//when the state should be fixed, the user could use a Query parameter to narrow down the Query
// Query<(something),user_supplied_filter>
//fn save<T: Component, Filter: bevy::ecs::query::QueryFilter = ()>(mut q: Query<(Entity, &Rollback<T>, &mut T), Filter>) {
//    //TODO:
//}

pub(crate) type DefaultFilter = ();    //With<RollbackID>;

//TODO: allow using Bundles, tuples, etc... for T, example: Rollback<(Transform, Velocity)>
//the default restore and save rollback systems, the user can use their own
pub fn restore<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(T::RestoreQuery<'_>, &Rollback<T>), DefaultFilter>,
    extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
) {
    restore_filter(info, query, extra);
}

pub fn restore_option<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(Entity, Option<T::RestoreQuery<'_>>, &Rollback<Option<T>>), DefaultFilter>,
    extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
    commands: Commands,
) {
    restore_option_filter(info, query, extra, commands);
}

pub fn save<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(T::SaveQuery<'_>, &mut Rollback<T>), DefaultFilter>,
    extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    save_filter(info, query, extra);
}

pub fn save_option<T: RollbackCapable>(
    info: Res<SnapshotInfo>,
    query: Query<(Option<T::SaveQuery<'_>>, &mut Rollback<Option<T>>), DefaultFilter>,
    extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    save_option_filter(info, query, extra);
}

pub fn restore_filter<T: RollbackCapable, Filter: QueryFilter>(
    info: Res<SnapshotInfo>,
    mut query: Query<(T::RestoreQuery<'_>, &Rollback<T>), Filter>,
    mut extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
) {
    for (q, r) in &mut query {
        r.0[info.current_index()].restore(q, &mut extra);
    }
}

pub fn restore_option_filter<T: RollbackCapable, Filter: QueryFilter>(
    info: Res<SnapshotInfo>,
    mut query: Query<(Entity, Option<T::RestoreQuery<'_>>, &Rollback<Option<T>>), Filter>,
    mut extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
    mut commands: Commands,
) {
    for (e, q, r) in &mut query {
        match (&r.0[info.current_index()], q) {
            (Some(to_restore), None) => to_restore.insert(e, &mut commands),
            (Some(to_restore), Some(q)) => to_restore.restore(q, &mut extra),
            (None, Some(_)) => T::remove(e, &mut commands),
            (None, None) => (),
        }
    }
}

pub fn save_filter<T: RollbackCapable, Filter: QueryFilter>(
    info: Res<SnapshotInfo>,
    mut query: Query<(T::SaveQuery<'_>, &mut Rollback<T>), Filter>,
    mut extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    for (q, mut r) in &mut query {
        r.0[info.current_index()] = T::save(q, &mut extra);
    }
}

pub fn save_option_filter<T: RollbackCapable, Filter: QueryFilter>(
    info: Res<SnapshotInfo>,
    mut query: Query<(Option<T::SaveQuery<'_>>, &mut Rollback<Option<T>>), Filter>,
    mut extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    for (q, mut r) in &mut query {
        r.0[info.current_index()] = q.map(|q | T::save(q, &mut extra));
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


//TODO: default systems that will take SnapshotUpdateEvent<T> that will simplify the usage

//TODO: default systems that will create some default interface that will simplify communication between client and server
// some reasonable defaults, refactor as much common work as possible out into this default system
// these systems could define a simple interface and basically tell the user what he should send across the connection
// and tell him what to do with anything that is received -> maybe do it through Events?