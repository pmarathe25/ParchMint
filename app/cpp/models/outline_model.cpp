#include "outline_model.h"

#include <QMetaObject>

OutlineModel::OutlineModel(QObject* parent)
  : QAbstractListModel(parent)
{
}

QObject* OutlineModel::source() const { return m_source; }

void OutlineModel::setSource(QObject* source)
{
  if (m_source == source)
    return;
  beginResetModel();
  m_source = source;
  endResetModel();
  emit sourceChanged();
}

int OutlineModel::rowCount(const QModelIndex& parent) const
{
  if (parent.isValid() || !m_source)
    return 0;
  return m_source->property("node_count").toInt();
}

QVariant OutlineModel::data(const QModelIndex& index, int role) const
{
  if (!index.isValid() || index.row() < 0 || index.row() >= rowCount())
    return {};
  switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
      return invoke("nodeTitle", index.row());
    case DepthRole:
      return invoke("nodeDepth", index.row());
    case ParentIdRole:
      return invoke("nodeParent", index.row());
    default:
      return {};
  }
}

QHash<int, QByteArray> OutlineModel::roleNames() const
{
  return {
    { TitleRole, QByteArrayLiteral("title") },
    { DepthRole, QByteArrayLiteral("depth") },
    { ParentIdRole, QByteArrayLiteral("parentId") },
  };
}

QVariant OutlineModel::invoke(const char* method, int row) const
{
  if (!m_source)
    return {};
  if (qstrcmp(method, "nodeTitle") == 0) {
    QString value;
    if (QMetaObject::invokeMethod(m_source, method, Qt::DirectConnection,
                                  Q_RETURN_ARG(QString, value), Q_ARG(qint32, row)))
      return value;
  } else {
    qint32 value = 0;
    if (QMetaObject::invokeMethod(m_source, method, Qt::DirectConnection,
                                  Q_RETURN_ARG(qint32, value), Q_ARG(qint32, row)))
      return value;
  }
  emit modelError(tr("Rust outline bridge method %1 failed.").arg(QString::fromLatin1(method)));
  return {};
}
