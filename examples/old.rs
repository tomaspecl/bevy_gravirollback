use gravirollback::old::*;

use bevy::prelude::*;
use bevy::ecs::query::WorldQuery;

fn main() {
    let mut app = App::new();

    app
    .add_plugins((
        DefaultPlugins,
        RollbackPlugin::default(),
    ))

    .add_systems(RollbackSchedule,
        (
            (
                transform_rollback_restore_system,
                visibility_rollback_restore_system,
            ).in_set(RollbackSet::RestoreState),
            
            rollback_save_system.in_set(RollbackSet::SaveState),
        )
    )
    //TODO: 
    ;
}

struct MyState {
    transform: Transform,
    visibility: Option<Visibility>,
}

/// Rollback entities with this will do rollback with [`Visibility`]
#[derive(Component)]
struct VisibilityRollback;

fn transform_rollback_restore_system(
    snapshot: Res<StateToRestore<MyState>>,
    mut query: Query<(Entity, &Rollback, &mut Transform)>,
    mut commands: Commands,
) {
    for (e,r,mut transform) in &mut query {
        if let Some(state) = snapshot.to_overwrite.get(r) {
            *transform = state.state.transform;
        }else{
            commands.entity(e).despawn_recursive();
        }
    }
}

fn visibility_rollback_restore_system(
    snapshot: Res<StateToRestore<MyState>>,
    mut query: Query<(Entity, &Rollback, &mut Visibility), With<VisibilityRollback>>,
    mut commands: Commands,
) {
    for (e,r,mut visibility) in &mut query {
        if let Some(state) = snapshot.to_overwrite.get(r) {
            *visibility = state.state.visibility
                .expect("let's keep it simple, no dynamic adding/removing of Rollback components from the entity");
        }else{
            commands.entity(e).despawn_recursive();
        }
    }
}

fn rollback_save_system(
    mut snapshot: ResMut<Snapshot<MyState>>,       //TODO: snapshot resource provided by RollbackSchedule will be empty
    mut query: Query<(&Rollback, &Transform, Option<&Visibility>)>,
) {
    for (&r,&transform,visibility) in &query {
        let visibility = visibility.map(|x| *x);
        snapshot.states.entry(r)
            .and_modify(|state| {
                state.state.transform = transform;
                state.state.visibility = visibility;
            })
            .or_insert(super::State {
                fixed: false,
                state: MyState { transform, visibility },
            });
    }
}

mod this_should_work {
    use super::*;
    use bevy::prelude::*;
    use bevy::ecs::query::WorldQuery;

    trait RollbackMarker {
        type Marker;
    }

    //TODO: this should work
    #[derive(RollbackMarker,WorldQuery)]
    #[world_query(mutable)]
    struct VisibilityRollback {
        visibility: &'static Visibility,
        //this could contain more stuff
    }
    //this should be generated:
    #[derive(Component)]
    struct VisibilityRollbackMarker;
    impl RollbackMarker for VisibilityRollback {
        type Marker = VisibilityRollbackMarker;
    }

    //TODO: make it possible to autogenerate this simple system from MyState like this perhaps
    #[derive(RollbackSystems)]
    struct MyState {
        #[rollback(transform_rollback_system)]  //-> will generate restore_transform_rollback_system and save_transform_rollback_system
        transform: Transform,
        #[rollback(visibility_rollback_system)]
        visibility: Option<VisibilityRollback>,
    }
    generate_rollback!{transform_rollback_system,Transform,MyState,transform} //this will generate this: (plus save_ variant)
    fn restore_transform_rollback_system(
        snapshot: Res<StateToRestore<MyState>>,
        mut query: Query<(Entity, &Rollback, &mut Transform)>,
        mut commands: Commands,
    ) {
        for (e,r,mut transform) in &mut query {
            if let Some(state) = snapshot.to_overwrite.get(r) {
                *transform = state.state.transform;
            }else{
                commands.entity(e).despawn_recursive();
            }
        }
    }
    generate_rollback!{system2,Transform,MyState}           //this will generate this: -> this will not compile because MyState != Transform
    fn restore_system2(
        snapshot: Res<StateToRestore<MyState>>,
        mut query: Query<(Entity, &Rollback, &mut Transform)>,
        mut commands: Commands,
    ) {
        for (e,r,mut transform) in &mut query {
            if let Some(state) = snapshot.to_overwrite.get(r) {
                *transform = state.state;
            }else{
                commands.entity(e).despawn_recursive();
            }
        }
    }
    //plus the save_ variant

    generate_rollback!{visibility_rollback_system,VisibilityRollback,MyState,visibility}
    
}
