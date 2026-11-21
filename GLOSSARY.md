# cleanX

cleanX makes disk space visible and removes only files it can justify. This glossary fixes the words we use for those two jobs.

## Language

**Orb**:
The central indicator of free disk space. It warms in color and glow as space runs out.
_Avoid_: gauge, ring, dial, meter

**Reclaimable space**:
The disk space that cleanX reports as safely removable from its allow-list.
_Avoid_: junk, garbage, debris

**Held space**:
Disk space still occupied by items cleanX has moved aside. It becomes free space only when the session's quarantine is cleared.
_Avoid_: pending space, staged space, freed space

**Freed space**:
Disk space actually returned to the volume.
_Avoid_: reclaimed space, saved space

**Impact tag**:
The label cleanX shows for how safe an item is to remove: safe, caution, or hands-off.
_Avoid_: risk badge, severity, warning level

**Safe clean**:
A removal limited to allow-listed items, confirmed in two steps.
_Avoid_: deep clean, purge, sweep

**Arming**:
The first of two presses. It prepares a safe clean; the second press removes the items.
_Avoid_: confirm, submit

**Owning tool**:
The package manager or application responsible for a cache. cleanX asks it to clean that cache instead of deleting files by hand.
_Avoid_: helper, backend

**Quarantine**:
A temporary holding area for removed items. cleanX keeps it until the session ends.
_Avoid_: recycle bin, trash, undo stack

**Scan**:
A read-only pass that measures disk use and finds reclaimable space. It never removes a file.
_Avoid_: sweep, clean, purge
