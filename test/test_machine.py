import home_guard  # noqa: F401  (first: every test on a throwaway home, never the real one)
import os
import sys
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(__file__)), "mods", "tasks", "bin"))
from herdr_task import machine

GIB = 2 ** 30


class MacSwapHold(unittest.TestCase):
    def test_high_swap_holds_even_with_room_in_memory(self):
        readings = {
            "hw.memsize": str(32 * GIB),
            "kern.memorystatus_level": "45",
            "vm.swapusage": "total = 18841,60M  used = 18329,60M  free = 512,00M  (encrypted)",
        }
        with mock.patch.object(machine, "MAC", True), \
                mock.patch.object(machine, "_run", side_effect=lambda argv: readings[argv[-1]]), \
                mock.patch.object(machine.os, "getloadavg", return_value=(0.0, 0.0, 0.0)), \
                mock.patch.object(machine, "_disks", return_value=[]):
            health = machine.sample()
        self.assertEqual(health["memory"], 55)
        self.assertEqual(machine.hold_reason(health), "swap is 97% used")
        self.assertIsNone(machine.hold_reason(health, {"start_swap": None}))

    def test_hold_uses_both_the_percentage_and_used_bytes(self):
        health = {"memory": 55, "load": 2, "cores": 12, "cpu": 17, "disk": 20}
        cases = [
            (0, 0, None),
            (GIB, GIB, None),  # A full small pool still allows starts.
            (5 * GIB, 4 * GIB, None),  # Exactly 80% is allowed.
            (5 * GIB, 4 * GIB + 2 ** 20, "swap is over 80% used"),  # Compare before rounding.
            (4 * GIB, 4 * GIB - 2 ** 20, None),  # Just below the used-byte floor.
            (4 * GIB, 4 * GIB, "swap is 100% used"),
        ]
        self.assertIsNone(machine.hold_reason(health))  # A sample without swap fields still works.
        for total, used, reason in cases:
            with self.subTest(total=total, used=used):
                self.assertEqual(machine.hold_reason({**health, "swap_total": total, "swap_used": used}), reason)

    def test_parser_reads_bytes_from_mac_sysctl(self):
        self.assertEqual(machine._parse_swapusage("total = 12.50G  used = 10240.25M  free = 2559.75M"),
                         (12 * GIB + GIB // 2, 10 * GIB + 2 ** 18))
        self.assertEqual(machine._parse_swapusage("total = 1024,00K  used = 256,50K"), (2 ** 20, 262656))
        for text in ("", "total = 512M", "total = 2M used = nope", "total = 1M used = 2M",
                     "total = 0.00M used = 0.00M"):
            with self.subTest(text=text):
                self.assertEqual(machine._parse_swapusage(text), (0, 0))
