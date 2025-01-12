use bevy::prelude::*;
use bevy::ecs::query::{WorldQuery, QueryData, QueryFilter};
use bevy::ecs::system::{StaticSystemParam, SystemParam};

use crate::*;

pub trait RollbackCapable: Send + Sync + 'static {
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

impl<T: Component + Clone> RollbackCapable for T
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

//so that the user can pick Rollback<Option<T>> or Rollback<T>
/*impl<T: RollbackCapable3> RollbackCapable3 for Option<T> {
    type QueryItem = Option<T>;
    type RestoreQuery<'a> = Option<T::RestoreQuery<'a>>;
}*/

//then user can do:
//register_rollback::<T>
//or
//register_rollback::<Option<T>>


//when the state should be fixed, the user could use a Query parameter to narrow down the Query
// Query<(something),user_supplied_filter>
//fn save<T: Component, Filter: bevy::ecs::query::QueryFilter = ()>(mut q: Query<(Entity, &Rollback<T>, &mut T), Filter>) {
//    //TODO:
//}

pub(crate) type DefaultFilter = ();    //With<RollbackID>;

//TODO: allow using Bundles, tuples, etc... for T, example: Rollback<(Transform, Velocity)>
//the default restore and save rollback systems, the user can use their own
pub fn restore<T: RollbackCapable, const LEN: usize>(
    current_frame: Res<Frame>,
    query: Query<(T::RestoreQuery<'_>, &Rollback<T, LEN>), DefaultFilter>,
    extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
) {
    restore_filter(current_frame, query, extra);
}

pub fn restore_option<T: RollbackCapable, const LEN: usize>(
    current_frame: Res<Frame>,
    query: Query<(Entity, Option<T::RestoreQuery<'_>>, &Rollback<Option<T>, LEN>), DefaultFilter>,
    extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
    commands: Commands,
) {
    restore_option_filter(current_frame, query, extra, commands);
}

pub fn save<T: RollbackCapable, const LEN: usize>(
    current_frame: Res<Frame>,
    query: Query<(T::SaveQuery<'_>, &mut Rollback<T, LEN>), DefaultFilter>,
    extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    save_filter(current_frame, query, extra);
}

pub fn save_option<T: RollbackCapable, const LEN: usize>(
    current_frame: Res<Frame>,
    query: Query<(Option<T::SaveQuery<'_>>, &mut Rollback<Option<T>, LEN>), DefaultFilter>,
    extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    save_option_filter(current_frame, query, extra);
}

pub fn restore_filter<T: RollbackCapable, const LEN: usize, Filter: QueryFilter>(
    current_frame: Res<Frame>,
    mut query: Query<(T::RestoreQuery<'_>, &Rollback<T, LEN>), Filter>,
    mut extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    for (q, r) in &mut query {
        r.0[current_index].restore(q, &mut extra);
    }
}

pub fn restore_option_filter<T: RollbackCapable, const LEN: usize, Filter: QueryFilter>(
    current_frame: Res<Frame>,
    mut query: Query<(Entity, Option<T::RestoreQuery<'_>>, &Rollback<Option<T>, LEN>), Filter>,
    mut extra: StaticSystemParam<T::RestoreExtraParam<'_>>,
    mut commands: Commands,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    for (e, q, r) in &mut query {
        match (&r.0[current_index], q) {
            (Some(to_restore), None) => to_restore.insert(e, &mut commands),
            (Some(to_restore), Some(q)) => to_restore.restore(q, &mut extra),
            (None, Some(_)) => T::remove(e, &mut commands),
            (None, None) => (),
        }
    }
}

pub fn save_filter<T: RollbackCapable, const LEN: usize, Filter: QueryFilter>(
    current_frame: Res<Frame>,
    mut query: Query<(T::SaveQuery<'_>, &mut Rollback<T, LEN>), Filter>,
    mut extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    for (q, mut r) in &mut query {
        r.0[current_index] = T::save(q, &mut extra);
    }
}

pub fn save_option_filter<T: RollbackCapable, const LEN: usize, Filter: QueryFilter>(
    current_frame: Res<Frame>,
    mut query: Query<(Option<T::SaveQuery<'_>>, &mut Rollback<Option<T>, LEN>), Filter>,
    mut extra: StaticSystemParam<T::SaveExtraParam<'_>>,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    for (q, mut r) in &mut query {
        r.0[current_index] = q.map(|q| T::save(q, &mut extra));
    }
}

//make the same systems for Resources
//the user can then use arbitrary types for storing Inputs and still have rollback work for them

pub fn restore_resource<T: Resource + Clone + Default, const LEN: usize>( //TODO: remove Default requirement
    current_frame: Res<Frame>,
    rollback: Res<Rollback<T, LEN>>,
    mut resource: ResMut<T>,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    *resource = rollback.0[current_index].clone();
}

pub fn restore_resource_option<T: Resource + Clone, const LEN: usize>(
    current_frame: Res<Frame>,
    rollback: Res<Rollback<Option<T>, LEN>>,
    resource: Option<ResMut<T>>,
    mut commands: Commands,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    if let Some(res) = &rollback.0[current_index] {
        if let Some(mut resource) = resource {
            *resource = res.clone();
        }else{
            commands.insert_resource(res.clone());
        }
    }else{
        commands.remove_resource::<T>();
    }
}

pub fn save_resource<T: Resource + Clone + Default, const LEN: usize>( //TODO: remove Default requirement
    current_frame: Res<Frame>,
    mut rollback: ResMut<Rollback<T, LEN>>,
    resource: Res<T>,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    rollback.0[current_index] = resource.clone();
}

pub fn save_resource_option<T: Resource + Clone, const LEN: usize>(
    current_frame: Res<Frame>,
    mut rollback: ResMut<Rollback<Option<T>, LEN>>,
    resource: Option<Res<T>>,
) {
    let current_index = crate::index::<LEN>(current_frame.0);
    rollback.0[current_index] = resource.map(|x| x.clone());
}

pub fn clear_resource_input_default<T: Resource + Default, const LEN: usize>(
    current_frame: Res<Frame>,
    last_frame: Res<LastFrame>,
    mut rollback: ResMut<Rollback<T, LEN>>,
) {
    if current_frame.0 > last_frame.0 {
        //this input is completely new, so it should be cleared
        let current_index = crate::index::<LEN>(current_frame.0);
        rollback.0[current_index] = default();
    }
}

pub fn clear_resource_input_option<T: Resource + Clone, const LEN: usize>(
    current_frame: Res<Frame>,
    last_frame: Res<LastFrame>,
    mut rollback: ResMut<Rollback<Option<T>, LEN>>,
) {
    if current_frame.0 > last_frame.0 {
        //this input is completely new, so it should be cleared
        let current_index = crate::index::<LEN>(current_frame.0);
        rollback.0[current_index] = None;
    }
}


//TODO: default systems that will take SnapshotUpdateEvent<T> that will simplify the usage

//TODO: default systems that will create some default interface that will simplify communication between client and server
// some reasonable defaults, refactor as much common work as possible out into this default system
// these systems could define a simple interface and basically tell the user what he should send across the connection
// and tell him what to do with anything that is received -> maybe do it through Events?