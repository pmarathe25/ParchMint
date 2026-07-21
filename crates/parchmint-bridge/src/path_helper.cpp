#include "path_helper.h"

#include <QDir>
#include <QStandardPaths>

QString parchmint_documents_location()
{
  const auto locations = QStandardPaths::standardLocations(QStandardPaths::DocumentsLocation);
  if (!locations.isEmpty())
    return QDir::cleanPath(locations.constFirst());
  return QDir::homePath();
}

QString parchmint_home_location()
{
  return QDir::homePath();
}
