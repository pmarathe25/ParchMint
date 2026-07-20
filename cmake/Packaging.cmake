# Engineering packages are produced for validation only. Publication remains
# disabled until the distribution-license ADR and release environment approve it.
set(CPACK_PACKAGE_NAME "ParchMint")
set(CPACK_PACKAGE_VENDOR "ParchMint")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "Local-first long-form writing application")
set(CPACK_PACKAGE_VERSION "${PROJECT_VERSION}")
set(CPACK_PACKAGE_INSTALL_DIRECTORY "ParchMint")
set(CPACK_PACKAGE_CHECKSUM "SHA256")
set(CPACK_RESOURCE_FILE_README "${CMAKE_SOURCE_DIR}/README.md")
set(CPACK_MONOLITHIC_INSTALL ON)

install(FILES
  "${CMAKE_SOURCE_DIR}/docs/legal/PRIVACY.md"
  "${CMAKE_SOURCE_DIR}/docs/legal/THIRD_PARTY_NOTICES.md"
  "${CMAKE_SOURCE_DIR}/docs/format/project-format-1.md"
  DESTINATION share/doc/parchmint)

if(WIN32)
  set(CPACK_GENERATOR "WIX;ZIP")
  set(CPACK_WIX_UPGRADE_GUID "7B9DA07E-B3BA-4E31-B4F6-187298B06D41")
elseif(APPLE)
  set(CPACK_GENERATOR "DragNDrop;TGZ")
  set(CPACK_DMG_VOLUME_NAME "ParchMint ${PROJECT_VERSION}")
else()
  include(GNUInstallDirs)
  install(FILES "${CMAKE_SOURCE_DIR}/packaging/linux/org.parchmint.ParchMint.desktop"
          DESTINATION "${CMAKE_INSTALL_DATADIR}/applications")
  install(FILES "${CMAKE_SOURCE_DIR}/packaging/linux/org.parchmint.ParchMint.metainfo.xml"
          DESTINATION "${CMAKE_INSTALL_DATADIR}/metainfo")
  install(FILES "${CMAKE_SOURCE_DIR}/packaging/linux/org.parchmint.ParchMint.xml"
          DESTINATION "${CMAKE_INSTALL_DATADIR}/mime/packages")
  install(FILES "${CMAKE_SOURCE_DIR}/packaging/icons/org.parchmint.ParchMint.svg"
          DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps")
  set(CPACK_GENERATOR "TGZ")
endif()

include(CPack)
