use super::*;
use super::systems::*;

use bevy::ecs::schedule::SystemConfigs;

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

pub fn getter<T: Clone + Default + 'static + Send + Sync>(entity: &mut EntityWorldMut) -> Option<Box<dyn RollbackStorage>> {    //TODO: remove Default requirement
    entity.take::<Rollback<T>>().map(|x| Box::new(x) as Box<dyn RollbackStorage>)
}

use std::marker::PhantomData;

pub struct RollbackConfig {
    pub systems: Vec<SystemConfigs>,
    pub getters: Vec<Getter>,
}
impl RollbackConfig {
    pub fn new() -> RollbackConfig {
        RollbackConfig {
            systems: Vec::new(),
            getters: Vec::new(),
        }
    }
    pub fn apply(self, app: &mut App) {
        for system in self.systems {
            app.add_systems(RollbackSchedule, system);
        }
        app.insert_resource(RollbackRegistry {
            getters: self.getters,
        });
        
    }

    pub fn register_component<T>(self) -> RollbackComponentConfig<T> {
        RollbackComponentConfig {
            config: self,
            restore_system: None,
            save_system: None,
            getter: None,
            _type: PhantomData,
        }
    }
}

pub struct RollbackComponentConfig<T> {
    config: RollbackConfig,
    restore_system: Option<SystemConfigs>,
    save_system: Option<SystemConfigs>,
    getter: Option<Getter>,
    _type: PhantomData<T>,
}
impl<T> RollbackComponentConfig<T> {
    pub fn finish(mut self) -> RollbackConfig {
        if let Some(getter) = self.getter {
            self.config.getters.push(getter);
        }else{
            warn!("Getter function was not set for RollbackComponentConfig<{}>",std::any::type_name::<T>());
        }
        if let Some(restore) = self.restore_system {
            self.config.systems.push(restore);
        }else{
            warn!("Restore function was not set for RollbackComponentConfig<{}>",std::any::type_name::<T>());
        }
        if let Some(save) = self.save_system {
            self.config.systems.push(save);
        }else{
            warn!("Save function was not set for RollbackComponentConfig<{}>",std::any::type_name::<T>());
        }
        self.config
    }
    pub fn set_restore<M>(mut self, restore_system: impl IntoSystemConfigs<M>) -> Self {
        self.restore_system = Some(restore_system.into_configs());
        self
    }
    pub fn set_save<M>(mut self, save_system: impl IntoSystemConfigs<M>) -> Self {
        self.save_system = Some(save_system.into_configs());
        self
    }
    pub fn set_getter(mut self, getter: Getter) -> Self {
        self.getter = Some(getter);
        self
    }
    pub fn register_component<T2>(self) -> RollbackComponentConfig<T2> {
        self.finish().register_component()
    }
}
impl<T: 'static + Send + Sync + Clone + Default> RollbackComponentConfig<T> { //TODO: remove Default requirement
    pub fn default_getter(mut self) -> Self {
        self.getter = Some(getter::<T>);
        self
    }
}
impl<T: Component + Clone + Default> RollbackComponentConfig<T> { //TODO: remove Default requirement
    pub fn default_systems(mut self) -> Self {
        self.restore_system = Some(restore::<T>.in_set(RollbackSet::Restore));
        self.save_system = Some(save::<T>.in_set(RollbackSet::Save));
        self
    }
}
impl<T: Component + Send + Sync + Clone + Default> RollbackComponentConfig<T> { //TODO: remove Default requirement
    pub fn defaults(self) -> Self {
        self.default_systems().default_getter()
    }
}

/*
this will be used in Plugin configuration

RollbackConfig::new()
    .register_component::<Transform>()
        .set_restore(my_transform_restore_system)
        .set_save(my_transform_save_system)
        .set_getter(my_getter)  //is this a good idea?
        .configure_something(...)
        .merging_policy(my_transform_merge_system)
    .register_resource::<PlayerScore>()
        .configure_something(...)
    .register_input::<MyInput>()
        .configure_something(...)
        .set_restore(my_input_restore_system)


OR

RollbackConfig::new(vec![
    register_component::<Transform>()
        .set_restore(my_transform_restore_system)
        .set_save(my_transform_save_system)
        .configure_something(...),
    register_resource::<PlayerScore>()
        .configure_something(...),
    register_input::<MyInput>()
        .configure_something(...)
        .set_restore(my_input_restore_system),
])
*/