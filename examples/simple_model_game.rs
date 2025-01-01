use bevy_gravirollback::systems::*;
use bevy_gravirollback::*;
use bevy_gravirollback::for_user::*;
use bevy_gravirollback::schedule_plugin::*;
use bevy_gravirollback::existence_plugin::*;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::PresentMode;

use bevy_inspector_egui::quick::WorldInspectorPlugin;

use std::time::Duration;

// This example shows a ball which is falling down and every time
// a signal is received the ball will move back up into its initial position
// and start falling from there again.
// This example will have a virtual sender that will send those signals
// at regular intervals but those signals will be artificialy delayed at random.
// The delay will cause the ball to appear to fall down further than it should.
// When the ball reaches a certain depth (which will only happen because of the delay),
// the ball will be despawned.
// But as soon as the delayed signals get received, the rollback will move back in
// time and replay those signals at their propper time. This will cause the ball
// to rewind and actually never reach the depth and therefore never despawn.

// This is a simple model of a multiplayer game where some internet trafic
// of one player (the ball) gets delayed but corrected by the rollback.
// The ball player is trying to not get low enough to despawn.
// From the perspective of the ball player he is doing everything correctly.
// Therefore there should be no way for him to trigger the despawn.
// As it would not be fair for the ball player to loose just because some
// packet got delayed it has to be corrected by the rollback, that is its job.

fn main() {
    let mut app = App::new();

    app.add_plugins((
        DefaultPlugins,
        WorldInspectorPlugin::new(),
        RollbackPlugin,
        RollbackSchedulePlugin::default(),
        ExistencePlugin,
    ))

    //TODO: these should be probably automaticaly registered
    .register_type::<RestoreStates>()
    .register_type::<RestoreInputs>()
    .register_type::<SaveStates>()
    .register_type::<SnapshotInfo>()
    .register_type::<RollbackMap>()
    //and these too
    .register_type::<RollbackID>()
    .register_type::<Exists>()
    .register_type::<Rollback<Exists>>()

    .register_type::<Rollback<Transform>>()

    .insert_resource(AmbientLight {
        color: Color::srgb(1.0,1.0,1.0),
        brightness: 0.2,
    })
    .insert_resource(ClearColor(Color::default()))

    .add_systems(Startup, setup)

    .insert_resource(UpdateTimer(Duration::from_secs(0)))
    .add_systems(Update,(
        advance_frame,
        get_input,
    ).in_set(RollbackProcessSet::HandleIO))

    .add_systems(RollbackSchedule,(     
        Transform::get_default_rollback_systems(),                                                              //
        restore_resource_option::<PlayerInput>.in_set(RollbackSet::RestoreInputs),                              //
        save_resource_input_option::<PlayerInput>.in_set(RollbackSet::Save),//more like clear_input, TODO: is it needed//
    ))                                                                                                          //  THIS SHOULD BE AUTOMATIC
    .insert_resource(Rollback::<Option<PlayerInput>>::default())
    
    .add_systems(RollbackSchedule,(
        (
            jump,
            fall,
            ball_existence,
        ).chain()
    ).in_set(RollbackSet::Update))

    .insert_resource(WaitingInputs(Vec::new()))
    ;

    app.run();
}

#[derive(Resource, Clone, Default)]  //TODO: remove Default requirement
struct PlayerInput;

#[derive(Component)]
struct BallMarker;

fn setup(
    mut commands: Commands,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    window.single_mut().present_mode = PresentMode::AutoNoVsync;

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 30.0)
    ));
    println!("running setup");
    let id = RollbackID(0);     //I should make sure its unique
    //commands.add(spawn(spawn_ball, (Transform::from_xyz(0.0, 10.0, 0.0), id)));
    //commands.add(spawn2(|world| spawn_ball2(Transform::from_xyz(0.0, 10.0, 0.0), id, world)));
    commands.queue(spawn3(spawn_ball3(Transform::from_xyz(0.0, 10.0, 0.0), id)));
    
    /*
    let t = Transform::default();
    commands.add(spawn(move |In(id), world| {
        world.spawn((id,t)).id()
    }, RollbackID(3)));

    commands.add(spawn2(move |world| {
        world.spawn((RollbackID(4),t)).id()
    }));


    #[derive(Component)]
    struct X;
    impl X {
        fn clon(&mut self) -> X { X }
    }

    let mut x = X;
    commands.add(spawn2(move |world| {
        world.spawn((RollbackID(5),t,x.clon())).id()
    }));

    let mut x = X;
    commands.add(spawn3(move |mut commands: Commands| {
        commands.spawn((RollbackID(6),t,x.clon())).id()
    }));
    */
}

fn spawn_ball(In((transform, id)): In<(Transform, RollbackID)>, world: &mut World) -> Entity {
    spawn_ball2(transform, id, world)
}

fn spawn_ball2(transform: Transform, id: RollbackID, world: &mut World) -> Entity {
    let mut assets = world.resource_mut::<Assets<Mesh>>();
    let mesh = assets.add(Sphere::default());

    println!("spawning ball");

    world.spawn(Mesh3d(mesh)).insert((
        BallMarker,
        id,
        Rollback::<Exists>::default(),
        make_rollback(transform),  // this will contain the snapshots of Transform for this entity
    )).id()
}

fn spawn_ball3(transform: Transform, id: RollbackID) -> impl Fn(Commands, ResMut<Assets<Mesh>>) -> Entity {
    move |mut commands, mut assets| {
        let mesh = assets.add(Sphere::default());

        println!("spawning ball");

        commands.spawn(Mesh3d(mesh)).insert((
            BallMarker,
            id,
            make_rollback(Exists(true)),
            make_rollback(transform),  // this will contain the snapshots of Transform for this entity
        )).id()
    }
}

fn fall(mut q: Query<(&mut Exists, &mut Transform), With<BallMarker>>) {
    let Ok((mut exists, mut transform)) = q.get_single_mut() else {return};
    transform.translation = transform.translation + transform.down() * 0.3;
    if transform.translation.y <= 0.0 {
        println!("despawning ball");
        exists.0 = false;
    }
}

fn ball_existence(mut q: Query<(&Exists, &mut Visibility), (With<BallMarker>, Changed<Exists>)>) {
    let Ok((exists, mut visibility)) = q.get_single_mut() else {return};
    if exists.0 {
        *visibility = Visibility::Visible;
    }else{
        *visibility = Visibility::Hidden;
    }
}

fn jump(mut q: Query<&mut Transform, With<BallMarker>>, input: Option<Res<PlayerInput>>) {
    if input.is_some() {
        let Ok(mut transform) = q.get_single_mut() else {return};
        transform.translation.y = 10.0;
    }
}

#[derive(Resource)]
struct UpdateTimer(Duration);

const FRAME_DELAY: u32 = 10;

#[derive(Resource)]
struct WaitingInputs(Vec<u64>);

fn advance_frame(
    time: Res<Time>,
    mut timer: ResMut<UpdateTimer>,
    mut info: ResMut<SnapshotInfo>,
    mut counter: Local<u32>,
    mut waiting: ResMut<WaitingInputs>,
) {
    let delay = 1000/60;

    if info.last==info.current {
        if time.elapsed() - timer.0 >= Duration::from_millis(delay) {
            timer.0 += Duration::from_millis(delay);

            print!("\nadvancing frame {} ",info.current);
            {   //this should be instead done by creating an event AdvanceFrameEvent or something like that
                let last_index = (info.last%SNAPSHOTS_LEN as u64) as usize;
                info.snapshots[last_index].modified = true;
            }
    
            *counter += 1;
            if *counter==FRAME_DELAY {
                *counter = 0;
    
                waiting.0.push(info.last);
            }
        }
    }
}

//TODO: this should be made by the library
#[derive(Event)]
struct NewInputEvent {
    frame: u64,
    data: (/*****/),
}

fn get_input(
    mut waiting: ResMut<WaitingInputs>,
    mut info: ResMut<SnapshotInfo>,
    mut inputs: ResMut<Rollback<Option<PlayerInput>>>,
    //mut events: EventWriter<NewInputEvent>,
) {
    let max = 15;
    
    let count = waiting.0.len();

    let flag = count>=max || rand::random::<f32>() < -0.1 + (count as f32) / max as f32;

    if flag && count!=0 {
        let frame = waiting.0.remove(0);
        
        //this should be done automaticaly:
        let index = info.index(frame);
        if info.snapshots[index].frame==frame {
            //insert the input
            inputs.0[index] = Some(PlayerInput);

            info.snapshots[index].modified = true;
        }else{
            todo!("dropped snapshot");
        }

        //this should be used instead:
        //events.send(NewInputEvent {
        //    frame,
        //    data: (),
        //});
    }
}