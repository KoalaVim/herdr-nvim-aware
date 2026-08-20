/*
 * herdr-nvim-aware -- nvim-aware keybinding actions for herdr.
 *
 * Generalization of herdr-nvim-nav: checks a marker file to determine whether
 * Neovim owns the focused pane. If yes, forwards the key to Neovim via
 * pane.send_keys. If no, performs the corresponding herdr action (navigate,
 * split, close, zoom).
 *
 * Build:  cc -O2 -o herdr-nvim-aware herdr-nvim-aware.c
 */

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#define PATH_BUF 1024
#define JSON_BUF 512

/* --- Marker detection --- */

static int build_marker_path(const char *pane, char *out, size_t out_len) {
    const char *cache = getenv("XDG_CACHE_HOME");
    if (cache && *cache)
        return snprintf(out, out_len, "%s/herdr/nvim-panes/%s", cache, pane) < (int)out_len;

    const char *home = getenv("HOME");
    if (!home || !*home)
        return 0;
    return snprintf(out, out_len, "%s/.cache/herdr/nvim-panes/%s", home, pane) < (int)out_len;
}

static int marker_says_vim(const char *pane) {
    char path[PATH_BUF];
    if (!build_marker_path(pane, path, sizeof path))
        return 0;

    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        return 0;

    char buf[32];
    ssize_t n = read(fd, buf, sizeof buf - 1);
    close(fd);
    if (n <= 0)
        return 0;
    buf[n] = '\0';

    long pid = strtol(buf, NULL, 10);
    if (pid <= 0)
        return 0;

    if (kill((pid_t)pid, 0) == 0 || errno == EPERM)
        return 1;

    unlink(path);
    return 0;
}

/* --- Socket communication --- */

static int socket_path(char *out, size_t out_len) {
    const char *sock = getenv("HERDR_SOCKET_PATH");
    if (sock && *sock)
        return snprintf(out, out_len, "%s", sock) < (int)out_len;

    const char *home = getenv("HOME");
    if (!home || !*home)
        return 0;
    return snprintf(out, out_len, "%s/.config/herdr/herdr.sock", home) < (int)out_len;
}

static int herdr_request(const char *json) {
    char path[PATH_BUF];
    if (!socket_path(path, sizeof path))
        return -1;

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof addr);
    addr.sun_family = AF_UNIX;
    if (strlen(path) >= sizeof addr.sun_path)
        return -1;
    memcpy(addr.sun_path, path, strlen(path) + 1);

    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0)
        return -1;
    if (connect(fd, (struct sockaddr *)&addr, sizeof addr) != 0) {
        close(fd);
        return -1;
    }

    size_t len = strlen(json), sent = 0;
    while (sent < len) {
        ssize_t w = write(fd, json + sent, len - sent);
        if (w < 0) {
            if (errno == EINTR)
                continue;
            close(fd);
            return -1;
        }
        sent += (size_t)w;
    }

    char reply[256];
    ssize_t r;
    do {
        r = read(fd, reply, sizeof reply - 1);
    } while (r < 0 && errno == EINTR);

    close(fd);
    if (r <= 0)
        return -1;

    reply[r] = '\0';
    if (strstr(reply, "\"error\"") != NULL) {
        fprintf(stderr, "herdr-nvim-aware: rejected: %s\n", reply);
        return -1;
    }
    return 0;
}

/* --- Action dispatch --- */

struct action {
    const char *name;
    const char *nvim_key;     /* key to send to nvim */
    const char *herdr_method; /* method when nvim is NOT running */
    const char *herdr_params; /* params format (with %s for pane_id where needed) */
};

static const struct action actions[] = {
    {"left",      "ctrl+h", "pane.focus_direction", "\"direction\":\"left\",\"pane_id\":\"%s\""},
    {"down",      "ctrl+j", "pane.focus_direction", "\"direction\":\"down\",\"pane_id\":\"%s\""},
    {"up",        "ctrl+k", "pane.focus_direction", "\"direction\":\"up\",\"pane_id\":\"%s\""},
    {"right",     "ctrl+l", "pane.focus_direction", "\"direction\":\"right\",\"pane_id\":\"%s\""},
    {"split_v",   "alt+e",  "pane.split",           "\"direction\":\"right\",\"pane_id\":\"%s\""},
    {"split_h",   "alt+o",  "pane.split",           "\"direction\":\"down\",\"pane_id\":\"%s\""},
    {"close",     "alt+w",  "pane.close",           "\"pane_id\":\"%s\""},
    {"quit",      "alt+q",  "pane.close",           "\"pane_id\":\"%s\""},
    {"zoom",      "alt+z",  "pane.zoom",            "\"pane_id\":\"%s\",\"mode\":\"toggle\""},
    {NULL, NULL, NULL, NULL}
};

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: herdr-nvim-aware <action>\n");
        return 2;
    }

    const struct action *act = NULL;
    for (const struct action *a = actions; a->name; a++) {
        if (strcmp(argv[1], a->name) == 0) {
            act = a;
            break;
        }
    }
    if (!act) {
        fprintf(stderr, "herdr-nvim-aware: unknown action: %s\n", argv[1]);
        return 2;
    }

    const char *pane = getenv("HERDR_PANE_ID");
    if (!pane)
        pane = "";

    char json[JSON_BUF];

    if (*pane && marker_says_vim(pane)) {
        snprintf(json, sizeof json,
                 "{\"id\":\"nvim-aware\",\"method\":\"pane.send_keys\","
                 "\"params\":{\"pane_id\":\"%s\",\"keys\":[\"%s\"]}}\n",
                 pane, act->nvim_key);
    } else if (*pane) {
        char params[256];
        snprintf(params, sizeof params, act->herdr_params, pane);
        snprintf(json, sizeof json,
                 "{\"id\":\"nvim-aware\",\"method\":\"%s\","
                 "\"params\":{%s}}\n",
                 act->herdr_method, params);
    } else {
        /* No pane ID -- build params without pane_id for focus_direction */
        if (strcmp(act->herdr_method, "pane.focus_direction") == 0) {
            snprintf(json, sizeof json,
                     "{\"id\":\"nvim-aware\",\"method\":\"pane.focus_direction\","
                     "\"params\":{\"direction\":\"%s\"}}\n",
                     argv[1]);
        } else {
            fprintf(stderr, "herdr-nvim-aware: no HERDR_PANE_ID for action %s\n", act->name);
            return 1;
        }
    }

    if (herdr_request(json) != 0) {
        fprintf(stderr, "herdr-nvim-aware: socket request failed\n");
        return 1;
    }
    return 0;
}
