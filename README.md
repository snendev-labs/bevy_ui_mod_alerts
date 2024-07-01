# bevy_ui_mod_alerts

A quick-and-dirty implementation of some ["toast" UI element](https://open-ui.org/components/toast.research/) represented by an `Alert` component. Call the `Alert::bundle` constructor to build an `AlertBundle`, or pipe a system that reutnrs `Vec<String>` into the `AlertsPlugin::alert` function, and a toast ui node will spawn (and eventually disappear if a lifetime is specified).

See examples for more.
