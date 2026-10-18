#pragma once

#include "f22_shim.h"
#include <dxgi.h>

namespace f22 {

// Installs DXGI Present and ResizeBuffers hooks
bool InstallDXGIHooks();
void RemoveDXGIHooks();

} // namespace f22
