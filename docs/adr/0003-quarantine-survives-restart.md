---
status: accepted
---

# Keep held items across restarts

Held items used to be purged at startup, so undo ended when the app closed. A manifest in the quarantine folder now rebuilds the held list at startup and the user clears it explicitly. A full cleaner can remove larger Caution items, so recovery must outlive the session. The cost is that held space stays occupied until the user clears it.
