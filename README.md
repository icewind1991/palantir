# Palantir

System metrics exporter for prometheus

## Exported metrics

- cpu, memory, gpu memory, io, network and disk usage stats
- zfs pool usage and arc stats
- cpu and gpu temperature
- cpu and gpu power usage on modern amd and intel platforms
- docker per-container cpu, memory and network stats

## Usage

- Download the binary for your architecture from the [releases](https://github.com/icewind1991/palantir/releases/) and place it at `/usr/local/bin/palantir`
- Place the [palantir.service](palantir.service) file in `/etc/systemd/system/`
- Create the `palantir` user: `sudo useradd -m palantir`
- Start enable enable the server: `sudo systemctl enable --now palantir`
- Metrics will be available at `localhost:5665/metrics`

Some stats require additional permissions described below.

## Power monitoring permissions

In recent kernel versions, precise power monitoring is only accessible to root users to prevent using it as a side-channel attack.
In order to get the power monitoring output you'll need to give the `palantir` user access to this data using the following steps.

- Create a group using
  
  ```bash
  sudo groupadd powermonitoring
  ```

- Create `/etc/udev/rules.d/99-powermonitoring.rules` with
  ```udev
  SUBSYSTEM=="powercap", ACTION=="add", RUN+="/bin/chgrp -R powermonitoring /sys%p", RUN+="/bin/chmod -R g=u /sys%p"
  SUBSYSTEM=="powercap", ACTION=="change", ENV{TRIGGER}!="none", RUN+="/bin/chgrp -R powermonitoring /sys%p", RUN+="/bin/chmod -R g=u /sys%p"
  ```

- Apply the udev rules
  
  ```
  sudo udevadm control --reload-rules && sudo udevadm trigger
  ```

- Add your user to the group
  
  ```
  sudo usermod -a -G powermonitoring palantir
  ```

- Verify that you can read energy usage

  ```
  sudo su - palantir -c 'cat /sys/class/powercap/intel-rapl:0:0/energy_uj'
  ```

## Docker monitoring permissions

To enable monitoring of docker containers, add the `palantir` user to the `docker` group

```bash
sudo usermod -a -G docker palantir
```

## Windows support

Palantir has limited windows support out of the box, additional sensors can be enabled by running [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor).