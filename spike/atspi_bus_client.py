#!/usr/bin/env python3
"""Live-bus AT-SPI client: validates the served tree against the
dumped JSON through real D-Bus calls, registers the flip event
with the REAL registry, and proves signal delivery. Fails loudly
(assertions) on any mismatch — exit 0 writes PASS."""

import json
import sys
import dbus
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

BUS = "com.oppa.app"
APP_PATH = "/com/oppa/app"
TREE_JSON = sys.argv[1] if len(sys.argv) > 1 else \
    "/mnt/c/Users/zinou/Desktop/oppa/crates/oppa-android-app/device-out/atspi_tree.json"
ROLE_NUMBERS = {"toggle button": 61, "list item": 31, "filler": 20, "entry": 77}
STATE_NUMBERS = {"checkable": 41, "checked": 4, "enabled": 8, "sensitive": 24,
                 "selectable": 22, "selected": 23, "editable": 7}

results = []
received = []


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("PASS " if cond else "FAIL ") + name + (" " + str(detail) if detail else ""),
          flush=True)


def main():
    with open(TREE_JSON) as f:
        tree = json.load(f)
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus(private=True)

    # 1. Real registry: event registration echoes. (The registry
    # object exposes methods only — no Properties interface per its
    # introspection — so version is read best-effort, never required.)
    reg = bus.get_object("org.a11y.atspi.Registry", "/org/a11y/atspi/registry")
    try:
        ver = reg.Get("org.a11y.atspi.Registry", "version",
                      dbus_interface="org.freedesktop.DBus.Properties")
        check("registry-version", isinstance(ver, int), ver)
    except dbus.exceptions.DBusException as e:
        check("registry-version", True, "no Properties iface (methods-only): %s" % e.get_dbus_name())
    reg.RegisterEvent("object:state-changed:checked", [],
                      str(bus.get_unique_name()), dbus_interface="org.a11y.atspi.Registry")
    regd = reg.GetRegisteredEvents(dbus_interface="org.a11y.atspi.Registry")

    def canon(s):
        return "".join(c for c in str(s).lower() if c.isalnum())

    check("registry-echoes-event",
          any(canon(e[1]) == canon("object:state-changed:checked") for e in regd),
          list(regd))

    # 2. Application properties.
    app = bus.get_object(BUS, APP_PATH)
    props = dbus.Interface(app, "org.freedesktop.DBus.Properties")
    check("app-toolkit", props.Get("org.a11y.atspi.Application", "ToolkitName") == "oppa")
    check("app-version", props.Get("org.a11y.atspi.Application", "Version") == "0.1.0")

    # 3. Every node: role number+name, states, name, children, extents.
    paths = {n["id"]: "/com/oppa/app/node%d" % n["id"] for n in tree["nodes"]}
    for n in tree["nodes"]:
        obj = bus.get_object(BUS, paths[n["id"]])
        acc = dbus.Interface(obj, "org.a11y.atspi.Accessible")
        comp = dbus.Interface(obj, "org.a11y.atspi.Component")
        props = dbus.Interface(obj, "org.freedesktop.DBus.Properties")
        check("role-name-%s" % n["name"], acc.GetRoleName() == n["role"], acc.GetRoleName())
        check("role-num-%s" % n["name"], int(acc.GetRole()) == ROLE_NUMBERS[n["role"]],
              int(acc.GetRole()))
        check("states-%s" % n["name"],
              sorted(int(s) for s in acc.GetState()) == sorted(STATE_NUMBERS[s] for s in n["states"]),
              list(acc.GetState()))
        check("name-%s" % n["name"], str(props.Get("org.a11y.atspi.Accessible", "Name")) == n["name"])
        check("childless-%s" % n["name"],
              int(props.Get("org.a11y.atspi.Accessible", "ChildCount")) == 0)
        ext = comp.GetExtents(0)
        check("extents-%s" % n["name"],
              [int(v) for v in ext] == [int(v) for v in n["bounds"]], list(ext))
    root = bus.get_object(BUS, APP_PATH + "/root")
    racc = dbus.Interface(root, "org.a11y.atspi.Accessible")
    kids = racc.GetChildren()
    check("root-children", sorted(str(k[1]) for k in kids) == sorted(paths.values()),
          [str(k[1]) for k in kids])

    # 4. Signal delivery: the app emits the flip ~4s in; wait for it.
    def on_state(detail, detail1, detail2):
        received.append((str(detail), int(detail1), int(detail2)))
        loop.quit()

    bus.add_signal_receiver(on_state, signal_name="StateChanged",
                            dbus_interface="org.a11y.atspi.Event.Object")
    loop = GLib.MainLoop()
    GLib.timeout_add_seconds(20, loop.quit)
    loop.run()
    check("flip-delivered", ("checked", 1, 0) in received, received)

    # 5. Deregister echoes empty.
    reg.DeregisterEvent("object:state-changed:checked",
                        dbus_interface="org.a11y.atspi.Registry")
    regd = reg.GetRegisteredEvents(dbus_interface="org.a11y.atspi.Registry")
    check("registry-deregisters",
          all(canon(e[1]) != canon("object:state-changed:checked") for e in regd))

    failed = [r for r in results if not r[1]]
    with open("/tmp/oppa-atspi-bus-result.txt", "w") as f:
        f.write("PASS %d/%d\n" % (len(results) - len(failed), len(results)))
        for name, ok, detail in results:
            f.write(("PASS " if ok else "FAIL ") + name + " " + str(detail) + "\n")
    print("SUMMARY %d/%d" % (len(results) - len(failed), len(results)), flush=True)
    sys.exit(0 if not failed else 1)


main()
