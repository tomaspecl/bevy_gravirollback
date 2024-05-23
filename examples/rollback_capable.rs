//this example is partial

use bevy_gravirollback::new::systems::*;
use bevy_gravirollback::new::*;
use bevy_gravirollback::new::for_user::*;

use bevy::prelude::*;
use bevy::ecs::system::StaticSystemParam;

/// Imagine that you have a point going in a circle and that movement is based on a parameter.
/// Then it is not needed to do [`Rollback`] on the whole [`Transform`] but only on that parameter.
/// 
/// This principle can be applied to any other case where some data transformation
/// needs to be done before restoring/saving the state.
/// The data Type used does not even need to `#[derive(Component)]`
/// and can only be stored in the `Rollback<Type>`.
/// This example used `#[derive(Component)]` as this `Parameter` would be stored on the entity
/// and there would be a system that would update the `Parameter` and `Transform` at the same time.
#[derive(Component, Default)]   //TODO: remove Default requirement
struct Parameter(f32);

impl RollbackCapable for Parameter {
    type RestoreQuery<'a> = (&'a mut Transform, &'a mut Parameter);
    type RestoreExtraParam<'a> = ();
    type SaveQuery<'a> = &'a Parameter;
    type SaveExtraParam<'a> = ();

    fn restore(&self, mut q: (Mut<Transform>, Mut<Parameter>), _extra: &mut StaticSystemParam<()>) {
        //update the `Transform`
        let pos = &mut q.0.translation;
        (pos.y, pos.x) = self.0.sin_cos();
        //update the `Parameter`
        q.1.0 = self.0;
    }
    fn save(q: &Parameter, _extra: &mut StaticSystemParam<()>) -> Self {
        Parameter(q.0)
    }
}

/// This can also be used for initializing/deinitializing rollback entities
/// -> probably not a good idea?
#[derive(Component)]
struct PlayerMarker;

//impl RollbackCapable for PlayerMarker {
//    type RestoreQuery<'a> = ();
//}

fn main() {}