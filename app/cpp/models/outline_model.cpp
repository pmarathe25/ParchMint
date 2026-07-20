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
  if (m_source)
    disconnect(m_source, nullptr, this, nullptr);
  if (m_revisionConnection)
    disconnect(m_revisionConnection);
  beginResetModel();
  m_source = source;
  endResetModel();
  if (m_source) {
    m_revisionConnection = connect(m_source, &QObject::destroyed, this, [this] { setSource(nullptr); });
    connect(m_source, SIGNAL(revisionChanged()), this, SLOT(refresh()));
  }
  emit sourceChanged();
}

void OutlineModel::refresh()
{
  beginResetModel();
  endResetModel();
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
    case IdRole:
      return invoke("nodeId", index.row());
    case DepthRole:
      return invoke("nodeDepth", index.row());
    case ParentIdRole:
      return invoke("nodeParent", index.row());
    case SynopsisRole:
      return invoke("nodeSynopsis", index.row());
    case StatusRole:
      return invoke("nodeStatus", index.row());
    case LabelRole:
      return invoke("nodeLabel", index.row());
    case GroupRole:
      return invoke("nodeIsGroup", index.row());
    case RootRole:
      return invoke("nodeIsRoot", index.row());
    default:
      return {};
  }
}

QHash<int, QByteArray> OutlineModel::roleNames() const
{
  return {
    { TitleRole, QByteArrayLiteral("title") },
    { IdRole, QByteArrayLiteral("nodeId") },
    { DepthRole, QByteArrayLiteral("depth") },
    { ParentIdRole, QByteArrayLiteral("parentId") },
    { SynopsisRole, QByteArrayLiteral("synopsis") },
    { StatusRole, QByteArrayLiteral("status") },
    { LabelRole, QByteArrayLiteral("label") },
    { GroupRole, QByteArrayLiteral("isGroup") },
    { RootRole, QByteArrayLiteral("isRoot") },
  };
}

QVariant OutlineModel::invoke(const char* method, int row) const
{
  if (!m_source)
    return {};
  if (qstrcmp(method, "nodeTitle") == 0 || qstrcmp(method, "nodeId") == 0
      || qstrcmp(method, "nodeSynopsis") == 0 || qstrcmp(method, "nodeStatus") == 0
      || qstrcmp(method, "nodeLabel") == 0) {
    QString value;
    if (QMetaObject::invokeMethod(m_source, method, Qt::DirectConnection,
                                  Q_RETURN_ARG(QString, value), Q_ARG(qint32, row)))
      return value;
  } else if (qstrcmp(method, "nodeIsGroup") == 0 || qstrcmp(method, "nodeIsRoot") == 0) {
    bool value = false;
    if (QMetaObject::invokeMethod(m_source, method, Qt::DirectConnection,
                                  Q_RETURN_ARG(bool, value), Q_ARG(qint32, row)))
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
