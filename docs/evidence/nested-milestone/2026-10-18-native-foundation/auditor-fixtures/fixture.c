
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <unistd.h>
int main(int argc, char **argv) {
    (void)argc;
    const char *name = strrchr(argv[0], '/');
    name = name ? name + 1 : argv[0];
    if (strstr(name, "creat"))
        syscall(SYS_creat, "/dev/nvidia-fluxwm-audit-missing", 0600);
    if (strstr(name, "vendor"))
        syscall(SYS_openat, AT_FDCWD, "/dev/nvidia-fluxwm-audit-missing", O_RDONLY);
    if (strstr(name, "console"))
        syscall(SYS_openat, AT_FDCWD, "/dev/console-fluxwm-audit-missing", O_RDONLY);
    if (strstr(name, "ioctl")) {
        int fd = open("/dev/null", O_RDONLY);
        ioctl(fd, 0x4b3a, 0); /* KDSETMODE family, harmless /dev/null fixture */
        close(fd);
    }
    if (strstr(name, "handle"))
        syscall(SYS_open_by_handle_at, -1, NULL, 0);
    if (strstr(name, "compat")) {
        long result;
        __asm__ volatile("int $0x80" : "=a"(result) : "0"(20) : "memory"); /* i386 getpid */
    }
    puts("{\"session_opened\":false,\"gpu_contexts_created\":0,\"native_compositor\":false}");
    return 0;
}
