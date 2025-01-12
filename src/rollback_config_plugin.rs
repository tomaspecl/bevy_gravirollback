use bevy::prelude::*;
use bevy::ecs::query::QueryFilter;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::schedule::SystemConfigs;
use bevy::ecs::intern::Interned;

use crate::*;
use crate::schedule_plugin::*;
use crate::systems::*;

pub struct RollbackSystemConfigurator<const LEN: usize> {
    pub restore: Option<Interned<dyn ScheduleLabel>>,
    pub save: Option<Interned<dyn ScheduleLabel>>,
    restore_systems: Vec<SystemConfigs>,
    save_systems: Vec<SystemConfigs>,
}

impl<const LEN: usize> Default for RollbackSystemConfigurator<LEN> {
    fn default() -> Self {
        Self {
            restore: Some(RollbackRestore.intern()),
            save: Some(RollbackSave.intern()),
            restore_systems: vec![],
            save_systems: vec![],
        }
    }
}

impl<const LEN: usize> RollbackSystemConfigurator<LEN> {
    /// You can use default() to use RollbackRestore and RollbackSave respectively
    pub fn new(restore: Option<impl ScheduleLabel>, save: Option<impl ScheduleLabel>) -> Self {
        Self {restore: restore.map(|x| x.intern()), save: save.map(|x| x.intern()), ..default()}
    }

    pub fn apply(&mut self, app: &mut App) {
        self.restore.map(|schedule| self.restore_systems.drain(..).for_each(|system| {app.add_systems(schedule, system);}));
        self.save.map(|schedule| self.save_systems.drain(..).for_each(|system| {app.add_systems(schedule, system);}));
    }

    pub fn add<T: RollbackCapableGroup>(&mut self) -> &mut Self {
        self.restore_systems.extend(T::get_restore::<LEN,DefaultFilter>());
        self.save_systems.extend(T::get_save::<LEN,DefaultFilter>());
        self
    }
    pub fn add_option<T: RollbackCapableGroup>(&mut self) -> &mut Self {
        self.restore_systems.extend(T::get_restore_option::<LEN,DefaultFilter>());
        self.save_systems.extend(T::get_save_option::<LEN,DefaultFilter>());
        self
    }
    pub fn add_filter<T: RollbackCapableGroup, Filter: QueryFilter + 'static>(&mut self) -> &mut Self {
        self.restore_systems.extend(T::get_restore::<LEN,Filter>());
        self.save_systems.extend(T::get_save::<LEN,Filter>());
        self
    }
    pub fn add_option_filter<T: RollbackCapableGroup, Filter: QueryFilter + 'static>(&mut self) -> &mut Self {
        self.restore_systems.extend(T::get_restore_option::<LEN,Filter>());
        self.save_systems.extend(T::get_save_option::<LEN,Filter>());
        self
    }
}

pub trait RollbackCapableGroup {
    fn get_restore<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs>;
    fn get_restore_option<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs>;
    fn get_save<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs>;
    fn get_save_option<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs>;
}

macro_rules! impl_rollback_capable_tuple_config {
    ($($T:ident),*) => {
        impl<$($T: RollbackCapable),*> RollbackCapableGroup for ($($T,)*) {
            fn get_restore<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs> {
                vec![$(systems::restore_filter::<$T,LEN,Filter>.into_configs(),)*]
            }
            fn get_restore_option<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs> {
                vec![$(systems::restore_option_filter::<$T,LEN,Filter>.into_configs(),)*]
            }
            fn get_save<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs> {
                vec![$(systems::save_filter::<$T,LEN,Filter>.into_configs(),)*]
            }
            fn get_save_option<const LEN: usize, Filter: QueryFilter + 'static>() -> Vec<SystemConfigs> {
                vec![$(systems::save_option_filter::<$T,LEN,Filter>.into_configs(),)*]
            }
        }
    };
}

use bevy_utils::all_tuples;
all_tuples!(impl_rollback_capable_tuple_config, 1, 15, T);

/*
pub trait RollbackSystems {
    fn get_default_rollback_systems<const LEN: usize>() -> SystemConfigs {
        Self::get_default_rollback_systems_filtered::<LEN, With<RollbackID>>()
    }
    fn get_default_rollback_systems_option<const LEN: usize>() -> SystemConfigs {
        Self::get_default_rollback_systems_option_filtered::<LEN, With<RollbackID>>()
    }
    fn get_default_rollback_systems_filtered<const LEN: usize, F: QueryFilter + 'static>() -> SystemConfigs;
    fn get_default_rollback_systems_option_filtered<const LEN: usize, F: QueryFilter + 'static>() -> SystemConfigs;
}

impl<T: RollbackCapable> RollbackSystems for T {
    fn get_default_rollback_systems_filtered<const LEN: usize, F: QueryFilter + 'static>() -> SystemConfigs {
        (
            systems::restore_filter::<T, LEN, F>.in_set(RollbackSet::Restore),
            systems::save_filter::<T, LEN, F>.in_set(RollbackSet::Save),
        ).into_configs()
    }
    fn get_default_rollback_systems_option_filtered<const LEN: usize, F: QueryFilter + 'static>() -> SystemConfigs {
        (
            systems::restore_option_filter::<T, LEN, F>.in_set(RollbackSet::Restore),
            systems::save_option_filter::<T, LEN, F>.in_set(RollbackSet::Save),
        ).into_configs()
    }
}
*/

//experimental work in progress
/*

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


pub fn getter<T: Clone + Default + 'static + Send + Sync>(entity: &mut EntityWorldMut) -> Option<Box<dyn RollbackStorage>> {    //TODO: remove Default requirement
    entity.take::<Rollback<T>>().map(|x| Box::new(x) as Box<dyn RollbackStorage>)
}

use std::marker::PhantomData;
use bevy::ecs::schedule::SystemConfigs;

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

*/

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