#ifndef DNR_SYSTEM_MONITOR_H
#define DNR_SYSTEM_MONITOR_H
#include <cstdint>
#include "laufey.h"

// Called on the platform UI thread (the macOS implementation also marshals).
// Bits: lock=1, suspend=2, resume=4, idle time query=8. No event unlocks a vault.
uint32_t DnrSetSystemMonitor(laufey_system_event_fn callback, void* data);
double DnrSystemIdleSeconds();  // negative means unavailable
#endif
