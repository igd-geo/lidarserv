import json
import os
import shutil
import subprocess
import sys

CGROUPS_BASE_PATH = "/sys/fs/cgroup/"
#DISK_SPEEDS_MIBPS = [4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256]
DISK_SPEEDS_MIBPS = [4, 6, 8, 11, 16, 23, 32, 45, 64, 91, 128, 181, 256]

class CGroup:

    def __init__(self, name):
        """
        Creates a new cgroup (v2) with the given name
        """
        path = os.path.join(CGROUPS_BASE_PATH, name)
        os.mkdir(path)
        self._path = path

    def configure(self, block: str, rbps: int, wbps: int):
        """
        Sets the limits for the disk read/write speed in this cgroup.
        :param block: The block device for which to apply the speed limit. In MAJ:MIN format (as in the
                      output from the lsblk command)
        :param rbps: read bytes per second
        :param wbps: write bytes per second
        """
        io_limits_file = os.path.join(self._path, "io.max")
        with open(io_limits_file, "wt") as f:
            f.write(f"{block} rbps={rbps} wbps={wbps}")

    def add_pid(self, pid: int):
        """
        Adds the process with the given pid to the cgroup.
        :param pid: pid of the process to put into the cgroup
        """
        procs_file = os.path.join(self._path, "cgroup.procs")
        with open(procs_file, "wt") as f:
            f.write(str(pid))

    def add_self(self):
        """
        Adds the current process to the cgroup.
        """
        pid = os.getpid()
        self.add_pid(pid)

    def cleanup(self):
        """
        Removes the cgroup from the system
        Continuing to use the cgroup after calling this method will result in an error.
        Only call this, once all processes in this cgroup have stopped.
        """
        os.rmdir(self._path)


def get_blockdevice(path):
    """
    Returns the block device (maj:min format), that the given path resides on.
    Returns None, if no matching block device was found.
    """

    # get the root of the file system, that path resides on
    df_out = subprocess.run(["df", "--output=target", path], capture_output=True)
    df_out.check_returncode()
    df_lines = df_out.stdout.splitlines(keepends=False)

    # parse output from df command.
    # first name only contains the column headers of the outputted table.
    # the second line will contain the root path of the fs
    assert len(df_lines) == 2
    fs_base_path = df_lines[1].strip().decode("UTF-8")

    # get all block devices
    lsblk_out = subprocess.run(["lsblk", "--list", "--json"], capture_output=True)
    lsblk_out.check_returncode()
    blocks = json.loads(lsblk_out.stdout)

    # search for a block device, that has the given file system in its mount points
    assert isinstance(blocks, dict)
    assert "blockdevices" in blocks
    devices = blocks["blockdevices"]
    assert isinstance(devices, list)
    for dev in devices:
        assert isinstance(dev, dict)
        assert "mountpoints" in dev
        mountpoints = dev["mountpoints"]
        assert isinstance(mountpoints, list)
        assert "maj:min" in dev
        maj_min = dev["maj:min"]
        assert isinstance(maj_min, str)
        for mnt in mountpoints:
            if isinstance(mnt, str) and mnt == fs_base_path:
                return maj_min

    # fallback if no block device was found
    # (can happen, if the file system is not backed by a block device. E.g. tmpfs file system. Or any fuse fs.)
    return None


def main():
    data_folder = "/home/tobias/Documents/studium/master/lidarserver/data/evaluation"
    shutil.copy("/home/tobias/Downloads/20210427_messjob/20210427_mess3/IAPS_20210427_162821.txt",
                "/tmp/IAPS_20210427_162821.txt")
    shutil.copy("/home/tobias/Downloads/20210427_messjob/20210427_mess3/trajectory.txt", "/tmp/trajectory.txt")
    os.putenv("LIDARSERV_POINTS_FILE", "/tmp/IAPS_20210427_162821.txt")
    os.putenv("LIDARSERV_TRAJECTORY_FILE", "/tmp/trajectory.txt")

    results = []
    for disk_speed_mibps in DISK_SPEEDS_MIBPS:
        print(f"Disk speed: {disk_speed_mibps} MiB/s", file=sys.stderr)
        disk_speed_bps = disk_speed_mibps * 1024 * 1024
        blk = get_blockdevice(data_folder)
        cgroup = CGroup("lidarserv")
        cgroup.configure(blk, disk_speed_bps, disk_speed_bps)
        with subprocess.Popen(
                ["/home/tobias/Documents/studium/master/lidarserver/target/release/evaluation", "simple"],
                stdout=subprocess.PIPE,
                env=None,
                preexec_fn=cgroup.add_self
        ) as proc:
            (stdout, stderr) = proc.communicate()
        cgroup.cleanup()
        output = json.loads(stdout)
        results.append({
            "disk_speed_mibps": disk_speed_mibps,
            "data": output
        })
    print(json.dumps(results))


if __name__ == '__main__':
    main()
