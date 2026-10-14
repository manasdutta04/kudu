# CMake toolchain addition for Windows builds: link the static C runtime (/MT), matching the
# prebuilt speech library. llama.cpp's CMake setup ignores /MT compiler flags (policy CMP0091),
# so the runtime has to be chosen here.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded" CACHE STRING "Static C runtime" FORCE)
