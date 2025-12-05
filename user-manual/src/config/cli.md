# Command Line Interface

```text
Motya: A reverse proxy from Prossimo

Usage: motya [OPTIONS]

Options:
      --validate-configs
          Validate all configuration data and exit
      --config-toml <CONFIG_TOML>
          Path to the configuration file in TOML format
      --config-kdl <CONFIG_KDL>
          Path to the configuration file in KDL format
      --threads-per-service <THREADS_PER_SERVICE>
          Number of threads used in the worker pool for EACH service
      --daemonize
          Should the server be daemonized after starting?
      --upgrade
          Should the server take over an existing server?
      --upgrade-socket <UPGRADE_SOCKET>
          Path to upgrade socket
      --pidfile <PIDFILE>
          Path to the pidfile, used for upgrade
  -h, --help
          Print help
```

## `--validate-configs`

Running Motya with this option will validate the configuration, and immediately exit
without starting any Services. A non-zero return code will be given when the configuration
fails validation.

## `--config-toml <CONFIG_TOML>`

Running Motya with this option will instruct Motya to load the configuration file from
the provided path. Cannot be used with `--config-kdl`.

## `--config-kdl <CONFIG_KDL>`

Running Motya with this option will instruct Motya to load the configuration file from
the provided path. Cannot be used with `--config-toml`.

## `--threads-per-service <THREADS_PER_SERVICE>`

Running Motya with this option will instruct Motya to use the given number of worker
threads per service.

## `--daemonize`

Running Motya with this option will cause Motya to fork after the creation of all
Services. The application will return once all Services have been started.

If this option is not provided, the Motya application will run until it is commanded
to stop or a fatal error occurs.

## `--upgrade`

Running Motya with this option will cause Motya to take over an existing Motya
server's open connections. See [Hot Reloading] for more information about this.

[Hot Reloading]: ../reloading.md

## `--upgrade-socket <UPGRADE_SOCKET>`

Running Motya with this option will instruct Motya to look at the provided socket
path for receiving active Listeners from the currently running instance.

This must be an absolute path. This option only works on Linux.

See [Hot Reloading] for more information about this.

## `--pidfile <PIDFILE>`

Running Motya with this option will set the path for the created pidfile when
the server is configured to daemonize.

This must be an absolute path.
