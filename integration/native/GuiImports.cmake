# Dual Linux builds keep backend code static and load system GUI libraries on demand.
enable_language(ASM)
find_package(Python3 REQUIRED COMPONENTS Interpreter)
find_program(DNR_READELF readelf REQUIRED)
set(import_manifest "common|$<TARGET_FILE:laufey_backend_common>\ncef|$<JOIN:$<TARGET_OBJECTS:dnr_bridge>,\ncef|>\n")
set(import_dependencies dnr_laufey laufey_backend_common)
# backend-common's pkg-config result is scoped to its subdirectory; GTK is
# already included in either backend's native library list above.
set(import_libraries ${CEF_NATIVE_LIBRARIES} ${WEBVIEW_NATIVE_LIBRARIES})
list(REMOVE_DUPLICATES import_libraries)
foreach(library IN LISTS import_libraries)
  if(library MATCHES "^(m|pthread|dl)$")
    continue()
  endif()
  unset(import_library CACHE)
  find_library(import_library NAMES ${library}
    HINTS ${CEF_NATIVE_LIBRARY_DIRS} ${WEBVIEW_NATIVE_LIBRARY_DIRS} REQUIRED)
  string(APPEND import_manifest "library|${import_library}\n")
  list(APPEND import_dependencies "${import_library}")
endforeach()
if(TARGET dnr_cef)
  string(APPEND import_manifest "cef|$<JOIN:$<TARGET_OBJECTS:dnr_cef>,\ncef|>\ncef|$<TARGET_FILE:CEF::Wrapper>\nlibrary|$<TARGET_FILE:CEF::Library>\n")
  list(APPEND import_dependencies dnr_cef CEF::Wrapper "$<TARGET_FILE:CEF::Library>")
endif()
if(TARGET dnr_webview)
  string(APPEND import_manifest "webview|$<JOIN:$<TARGET_OBJECTS:dnr_webview>,\nwebview|>\n")
  list(APPEND import_dependencies dnr_webview)
endif()
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/gui-inputs" CONTENT "${import_manifest}")
add_custom_command(
  OUTPUT "${CMAKE_BINARY_DIR}/gui_trampolines.S" "${CMAKE_BINARY_DIR}/gui_imports.inc"
         "${CMAKE_BINARY_DIR}/gui-wrap-flags" "${CMAKE_BINARY_DIR}/gui-imports.json"
  COMMAND ${Python3_EXECUTABLE} "${CMAKE_CURRENT_SOURCE_DIR}/generate_gui_imports.py"
    --manifest "${CMAKE_BINARY_DIR}/gui-inputs" --output "${CMAKE_BINARY_DIR}"
    --readelf "${DNR_READELF}"
  DEPENDS ${import_dependencies} "${CMAKE_BINARY_DIR}/gui-inputs" generate_gui_imports.py
  VERBATIM)
add_library(dnr_gui_imports STATIC gui_loader.cc "${CMAKE_BINARY_DIR}/gui_trampolines.S"
  "${CMAKE_BINARY_DIR}/gui_imports.inc")
target_include_directories(dnr_gui_imports PRIVATE "${CMAKE_BINARY_DIR}")
target_compile_definitions(dnr_bridge PRIVATE DNR_LAZY_GUI=1)
install(TARGETS dnr_gui_imports ARCHIVE DESTINATION lib)
