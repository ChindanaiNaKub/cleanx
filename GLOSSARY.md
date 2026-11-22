# cleanX

cleanX makes disk space visible and removes files it can justify, with clear limits on what it never touches. This glossary fixes the words we use for those two jobs.

## Language

**Orb**:
The central indicator of free disk space. It warms in color and glow as space runs out.
_Avoid_: gauge, ring, dial, meter

**Reclaimable space**:
The disk space that cleanX reports as removable under its rules. It covers home only, so it is not the whole volume's used space.
_Avoid_: junk, garbage, debris, selected space

**Selected space**:
The part of reclaimable space the user has checked for the next clean.
_Avoid_: reclaimable space, chosen space

**Used space**:
The disk space reported as occupied on a whole volume, including files outside home. It is not the sum of the categories.
_Avoid_: reclaimable space, category total

**Held space**:
Disk space still occupied by items cleanX has moved aside. It survives a restart and becomes free space only when the user clears the quarantine.
_Avoid_: pending space, staged space, freed space

**Freed space**:
Disk space actually returned to the volume, including held items the user clears.
_Avoid_: reclaimed space, saved space

**Impact tag**:
The label cleanX shows for how safe an item is to remove: safe, caution, or hands-off. Each tag fixes the action: safe and caution are cleanable, hands-off is not.
_Avoid_: risk badge, severity, warning level

**Protected folder**:
A home folder cleanX reports but never offers to clean, such as `.config` or `.ssh`.
_Avoid_: blacklist, excluded path

**Safe clean**:
A two-press removal of reclaimable items. It never touches a protected folder.
_Avoid_: deep clean, purge, sweep

**Arming**:
The first of two presses. It prepares a safe clean; the second press removes the items.
_Avoid_: confirm, submit

**Owning tool**:
The package manager or application responsible for a cache. cleanX asks it to clean that cache instead of deleting files by hand.
_Avoid_: helper, backend

**Quarantine**:
A holding area for removed items. cleanX keeps it until the user clears it.
_Avoid_: recycle bin, trash, undo stack

**Scan**:
A read-only pass that measures disk use and finds reclaimable space. It never removes a file.
_Avoid_: sweep, clean, purge
