use crate::*;

// this file contains helper functions and structs for the library user

pub fn make_rollback<T: Component + Default>(component: T) -> (T, Rollback<T>) {
    (component, Rollback::default())
}

/// Used like this: commands.add(spawn(spawn_my_entity, (arguments)))
pub fn spawn<T: 'static + Clone + Send + Sync>(mut spawn_func: impl FnMut(In<T>, &mut World) -> Entity + 'static + Send + Sync, spawn_data: T) -> impl FnOnce(&mut World) {
    let mut system = move |world: &mut World| spawn_func(In(spawn_data.clone()), world);

    move |world: &mut World| {
        let _entity = system(world);
    }
}

//maybe this is more ergonomic?
pub fn spawn2(mut spawn_system: impl FnMut(&mut World) -> Entity + 'static + Send + Sync) -> impl FnOnce(&mut World) {
    move |world: &mut World| {
        let _entity = spawn_system(world);
    }
}

pub fn spawn3<M>(spawn_system: impl IntoSystem<(), Entity, M>) -> impl FnOnce(&mut World) {
    let mut spawn_system = Box::new(IntoSystem::into_system(spawn_system));
    move |world: &mut World| {
        spawn_system.initialize(world);
        let _entity = spawn_system.run((), world);
        spawn_system.apply_deferred(world);
    }
}
