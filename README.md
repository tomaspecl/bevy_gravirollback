# bevy_gravirollback

**This library is work in progress**
This library is a Plugin for the [Bevy engine](https://github.com/bevyengine/bevy) and can be used to implement rollback. I made this library mainly for my game project [GraviShot](https://github.com/tomaspecl/gravishot) (the code used to be part of the game and I split it off and refactored) but I decided that it might become useful to others one day.

## What is it supposed to do?
* Save specified components of specified entities (and resources) each game frame
* Whenever a frame in the past is modified it will be reloaded and resimulated - modifications can be changes of inputs or any other changes caused by perhaps receiving some messages from a server - like syncing
* This plugin will _not_ handle networking (at least for now) and all nececery messages will have to be handled and sent by the user of this plugin. I might add some default networking features in the future.