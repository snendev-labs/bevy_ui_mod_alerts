# bevy_ui_mod_alerts

[![Crates.io](https://img.shields.io/crates/v/bevy_ui_mod_alerts.svg)](https://crates.io/crates/bevy_ui_mod_alerts)
[![Docs](https://docs.rs/bevy_ui_mod_alerts/badge.svg)](https://docs.rs/bevy_ui_mod_alerts/latest/)

A quick-and-dirty implementation of some ["toast" UI element](https://open-ui.org/components/toast.research/) represented by an `Alert` component. Call the `Alert::bundle` constructor to build an `AlertBundle`, or pipe a system that reutnrs `Vec<String>` into the `AlertsPlugin::alert` function, and a toast ui node will spawn (and eventually disappear if a lifetime is specified).

See examples for more.
