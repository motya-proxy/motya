# Introduction

This is the user/operator facing manual for the Motya reverse proxy application.

Motya is a reverse proxy application under development, utilizing the `pingora` reverse proxy engine
from Cloudflare. It is written in the Rust language. It is configurable, allowing for options
including routing, filtering, and modification of proxied requests.

Motya acts as a binary distribution of the `pingora` engine - providing a typical application
interface for configuration and customization for operators.

The source code and issue tracker for Motya can be found [on GitHub]

[on GitHub]: https://github.com/memorysafety/river

For developer facing documentation, including project roadmap and feature requirements for the
1.0 release, please refer to the `docs/` folder [on GitHub].
