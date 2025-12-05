## From River to Motya

`Motya` started as a fork of the [River](https://github.com/memorysafety/river) project, but has since evolved into its own beast. 

At this point, the codebase has been almost entirely rewritten and refactored. Because almost nothing remains of the original source, I decided to rename the project to reflect its new identity *(and also to avoid the thick layer of digital dust settling on the original codebase).*

### Roadmap & Features
While I respect the architectural vision and feature milestones laid out by the original creators, **Motya follows its own path**. 
I will be implementing features from their roadmap, but **not** in their intended order. Instead, I plan to cherry-pick features based on necessity, curiosity, or whatever I feel like coding on a Friday night.

## Current State

Motya is currently v0.5.0. See the [v0.5.0 release notes] for more details on recently
added features.

[v0.5.0 release notes]: https://github.com/memorysafety/river/blob/main/docs/release-notes/2024-08-30-v0.5.0.md

**Until further notice, there is no expectation of stability.**

### Demonstration steps

At the moment, `motya` can be invoked from the command line. See `--help` for
all options.

Configuration is currently done exclusively via configuration file. See
[`test-config.kdl`] for an example configuration file. Additionally, see
[kdl configuration] for more configuration details.

[`test-config.kdl`]: ./source/motya/assets/test-config.kdl
[kdl configuration]: https://onevariable.com/motya-user-manual/config/kdl.html

## License

Licensed under the Apache License, Version 2.0: ([LICENSE-APACHE](./LICENSE-APACHE)
or <http://www.apache.org/licenses/LICENSE-2.0>).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
