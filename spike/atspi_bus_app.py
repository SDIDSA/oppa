#!/usr/bin/env python3
"""Live-bus AT-SPI app (test harness): serves the dumped AtspiTree
over the real a11y bus with at-spi2-canonical interface/member
shapes (verified against the running registry's introspection),
then emits the recorded flip signal once so the client can prove
delivery. Role/state NUMBERS come from the at-spi2 ABI header
order (append-only practice); role/state NAME strings are the
emitter's exact vocabulary under test."""

import json
import sys
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

BUS = "com.oppa.app"
APP_PATH = "/com/oppa/app"
TREE_JSON = sys.argv[1] if len(sys.argv) > 1 else \
    "/mnt/c/Users/zinou/Desktop/oppa/crates/oppa-android-app/device-out/atspi_tree.json"

ROLE_NUMBERS = {"toggle button": 61, "list item": 31, "filler": 20, "entry": 77}
STATE_NUMBERS = {"checkable": 41, "checked": 4, "enabled": 8, "sensitive": 24,
                 "selectable": 22, "selected": 23, "editable": 7}


class Application(dbus.service.Object):
    @dbus.service.method("org.freedesktop.DBus.Properties",
                         in_signature="ss", out_signature="v")
    def Get(self, interface, name):
        if interface == "org.a11y.atspi.Application" and name == "ToolkitName":
            return "oppa"
        if interface == "org.a11y.atspi.Application" and name == "Version":
            return "0.1.0"
        raise dbus.exceptions.DBusException(
            "org.freedesktop.DBus.Error.UnknownProperty", "no such property")

    @dbus.service.method("org.a11y.atspi.Application", in_signature="u", out_signature="s")
    def GetLocale(self, lctype):
        return "en_US"


class Accessible(dbus.service.Object):
    def __init__(self, bus, path, node, children):
        super().__init__(bus, path)
        self.node = node
        self.children = children

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="u")
    def GetRole(self):
        return ROLE_NUMBERS[self.node["role"]]

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="s")
    def GetRoleName(self):
        return self.node["role"]

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="au")
    def GetState(self):
        return dbus.Array([STATE_NUMBERS[s] for s in self.node["states"]], signature="u")

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="a(so)")
    def GetChildren(self):
        return dbus.Array(
            [dbus.Struct((BUS, c), signature="(so)") for c in self.children],
            signature="(so)")

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="i")
    def GetIndexInParent(self):
        return self.node.get("index", 0)

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="(so)")
    def GetApplication(self):
        return dbus.Struct((BUS, APP_PATH), signature="(so)")

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="a{ss}")
    def GetAttributes(self):
        return dbus.Dictionary({}, signature="ss")

    @dbus.service.method("org.a11y.atspi.Accessible", out_signature="as")
    def GetInterfaces(self):
        return ["Accessible", "Component"]

    @dbus.service.method("org.freedesktop.DBus.Properties",
                         in_signature="ss", out_signature="v")
    def Get(self, interface, name):
        if interface == "org.a11y.atspi.Accessible" and name == "Name":
            return self.node["name"]
        if interface == "org.a11y.atspi.Accessible" and name == "ChildCount":
            return dbus.Int32(len(self.children))
        if interface == "org.a11y.atspi.Accessible" and name == "Parent":
            return dbus.Struct((BUS, self.node["parent"]), signature="(so)")
        raise dbus.exceptions.DBusException(
            "org.freedesktop.DBus.Error.UnknownProperty", "no such property")

    @dbus.service.method("org.a11y.atspi.Component", in_signature="u", out_signature="(iiii)")
    def GetExtents(self, coord_type):
        b = self.node["bounds"]
        return dbus.Struct((int(b[0]), int(b[1]), int(b[2]), int(b[3])), signature="(iiii)")

    @dbus.service.signal("org.a11y.atspi.Event.Object", signature="sii")
    def StateChanged(self, detail, detail1, detail2):
        pass


def main():
    with open(TREE_JSON) as f:
        tree = json.load(f)
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus(private=True)
    bus.request_name(BUS)
    Application(bus, APP_PATH)
    paths = {}
    ROOT_PATH = APP_PATH + "/root"
    for i, n in enumerate(tree["nodes"]):
        path = "/com/oppa/app/node%d" % n["id"]
        n["parent"] = ROOT_PATH
        n["index"] = i
        paths[n["id"]] = path
    objs = {}
    for n in tree["nodes"]:
        objs[n["id"]] = Accessible(bus, paths[n["id"]], n, [])
    # App root exposes the flat child list (dump carries no hierarchy).
    root_children = [paths[n["id"]] for n in tree["nodes"]]
    root_node = {"id": -1, "role": "filler", "name": "oppa",
                 "states": ["enabled", "sensitive"],
                 "bounds": [0, 0, 300, 200], "parent": APP_PATH, "index": 0}
    root = Accessible(bus, ROOT_PATH, root_node, root_children)

    loop = GLib.MainLoop()

    def emit_flip():
        # The recorded flip: toggle (first node) checked on.
        objs[tree["nodes"][0]["id"]].StateChanged("checked", 1, 0)
        print("EMITTED flip", flush=True)
        return False

    def quit():
        loop.quit()
        return False

    GLib.timeout_add_seconds(4, emit_flip)
    GLib.timeout_add_seconds(40, quit)
    print("SERVING", flush=True)
    loop.run()


main()
