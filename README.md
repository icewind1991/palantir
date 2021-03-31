## Power monitoring permissions

In recent kernel versions, precise power monitoring is only accessible to root users to prevent using it as a side-channel attack.
In order to get the power monitoring output you'll need to give the palantir user access to this data using the following steps.

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
  sudo usermod -a G powermonitoring palantir
  ```

- Verify that you can read energy usage

  ```
  sudo su - palantir -c 'cat /sys/class/powercap/intel-rapl:0:0/energy_uj
  ```
