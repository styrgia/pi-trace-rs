# Spec Delta

## Purpose

Deliver the local application as one installable package and let Pi users control its lifecycle explicitly without changing their agent sessions or trace data.

## ADDED Requirements

### Requirement: Ship one application archive
The package SHALL contain the built web interface, a Rust executable for the build platform, the Pi extension, startup scripts and licenses. The web interface SHALL not require a second published npm package. Included Rust sources SHALL allow an explicit build when no matching bundled executable exists.

#### Scenario: Extracted package on the build platform
- **WHEN** the archive is installed outside the source checkout on its build platform
- **THEN** the application serves its interface and health endpoint without a compiler or frontend build tools

### Requirement: Keep installation and discovery passive
Installing or discovering the Pi package SHALL not compile the application, start a server, open a browser or read trace files. It SHALL register the documented application command.

#### Scenario: Fresh Pi resource discovery
- **WHEN** Pi discovers the installed extension
- **THEN** the command is registered and no child processes or application files are created

### Requirement: Manage only owned processes
The Pi command SHALL support explicit build, start and stop operations. Repeated starts SHALL reuse the same owned process. An occupied port SHALL report an error without attaching to or terminating an unrelated server. Session shutdown SHALL stop and await owned resources.

#### Scenario: Another service occupies the port
- **WHEN** a user starts the app while another service owns its port
- **THEN** the command reports the collision and leaves the other service running

#### Scenario: Pi reload
- **WHEN** the extension's session shuts down during reload
- **THEN** its application and build processes are terminated with bounded cleanup

### Requirement: Remain usable when browser opening fails
A successful server start SHALL report its local URL. Failure to open the default browser SHALL leave the owned server available and provide a manual URL.

#### Scenario: No desktop browser opener
- **WHEN** the server becomes ready but the browser opener fails
- **THEN** the user receives the URL and can open the app manually
