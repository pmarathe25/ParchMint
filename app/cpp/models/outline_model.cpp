#include "outline_model.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaObject>

namespace {
constexpr auto NodeMimeType = "application/x-parchmint-node-id";
}

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
  m_rows.clear();
  m_rowCount = m_source ? m_source->property("node_count").toInt() : 0;
  endResetModel();
  if (m_source) {
    connect(m_source, &QObject::destroyed, this, [this] { setSource(nullptr); });
    m_revisionConnection = connect(m_source,
                                   SIGNAL(outlineModelDelta(QString,int,int,int)),
                                   this,
                                   SLOT(applyDelta(QString,int,int,int)));
  }
  emit sourceChanged();
}

void OutlineModel::applyDelta(const QString& kind, int first, int destination, int count)
{
  count = qMax(0, count);
  if (kind == QStringLiteral("insert") && count > 0) {
    beginInsertRows({}, first, first + count - 1);
    m_rowCount += count;
    m_rows.clear();
    endInsertRows();
  } else if (kind == QStringLiteral("remove") && count > 0) {
    beginRemoveRows({}, first, first + count - 1);
    m_rowCount = qMax(0, m_rowCount - count);
    m_rows.clear();
    endRemoveRows();
  } else if (kind == QStringLiteral("move") && count > 0 && first != destination) {
    const int destinationChild = destination > first ? destination + count : destination;
    if (beginMoveRows({}, first, first + count - 1, {}, destinationChild)) {
      m_rows.clear();
      endMoveRows();
    }
  } else if (kind == QStringLiteral("data") && count > 0) {
    for (int row = first; row < first + count; ++row)
      m_rows.remove(row);
    emit dataChanged(index(first), index(qMin(m_rowCount - 1, first + count - 1)));
  } else if (kind == QStringLiteral("reset")) {
    beginResetModel();
    m_rows.clear();
    m_rowCount = m_source ? m_source->property("node_count").toInt() : 0;
    endResetModel();
  }
}

int OutlineModel::rowCount(const QModelIndex& parent) const
{
  if (parent.isValid() || !m_source)
    return 0;
  return m_rowCount;
}

QVariant OutlineModel::data(const QModelIndex& index, int role) const
{
  if (!index.isValid() || index.row() < 0 || index.row() >= rowCount())
    return {};
  const auto* row = cachedRow(index.row());
  if (!row)
    return {};
  switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
      return row->title;
    case IdRole:
      return row->id;
    case DepthRole:
      return row->depth;
    case ParentIdRole:
      return row->parent;
    case SynopsisRole:
      return row->synopsis;
    case StatusRole:
      return row->status;
    case LabelRole:
      return row->label;
    case GroupRole:
      return row->group;
    case RootRole:
      return row->root;
    case WordCountRole:
      return row->words;
    case IncludeInCompileRole:
      return row->include;
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
    { WordCountRole, QByteArrayLiteral("wordCount") },
    { IncludeInCompileRole, QByteArrayLiteral("includeInCompile") },
  };
}

Qt::ItemFlags OutlineModel::flags(const QModelIndex& index) const
{
  if (!index.isValid())
    return Qt::ItemIsDropEnabled;
  const bool root = data(index, RootRole).toBool();
  auto result = Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsDropEnabled;
  if (!root)
    result |= Qt::ItemIsDragEnabled;
  return result;
}

QStringList OutlineModel::mimeTypes() const { return { QString::fromLatin1(NodeMimeType) }; }

QMimeData* OutlineModel::mimeData(const QModelIndexList& indexes) const
{
  if (indexes.isEmpty())
    return nullptr;
  const auto index = indexes.constFirst();
  if (!index.isValid() || data(index, RootRole).toBool())
    return nullptr;
  auto* mime = new QMimeData;
  mime->setData(NodeMimeType, data(index, IdRole).toString().toUtf8());
  return mime;
}

bool OutlineModel::dropMimeData(const QMimeData* mime,
                                Qt::DropAction action,
                                int row,
                                int,
                                const QModelIndex& parent)
{
  if (action == Qt::IgnoreAction)
    return true;
  if (action != Qt::MoveAction || !mime || !mime->hasFormat(NodeMimeType) || !m_source
      || rowCount() == 0)
    return false;
  const auto sourceId = QString::fromUtf8(mime->data(NodeMimeType));
  const int targetRow = parent.isValid() ? parent.row() : qBound(0, row, rowCount() - 1);
  const auto target = index(targetRow, 0);
  if (!target.isValid())
    return false;
  bool moved = false;
  return QMetaObject::invokeMethod(m_source,
                                   "moveNode",
                                   Qt::DirectConnection,
                                   Q_RETURN_ARG(bool, moved),
                                   Q_ARG(QString, sourceId),
                                   Q_ARG(QString, data(target, IdRole).toString()),
                                   Q_ARG(QString, QStringLiteral("before")))
    && moved;
}

const OutlineModel::CachedRow* OutlineModel::cachedRow(int row) const
{
  if (!m_source)
    return nullptr;
  const auto existing = m_rows.constFind(row);
  if (existing != m_rows.cend())
    return &existing.value();
  QString payload;
  if (!QMetaObject::invokeMethod(m_source,
                                 "nodeRowJson",
                                 Qt::DirectConnection,
                                 Q_RETURN_ARG(QString, payload),
                                 Q_ARG(qint32, row))) {
    emit modelError(tr("Rust outline row bridge failed."));
    return nullptr;
  }
  const auto object = QJsonDocument::fromJson(payload.toUtf8()).object();
  if (object.isEmpty()) {
    emit modelError(tr("Rust outline row payload is invalid."));
    return nullptr;
  }
  CachedRow value;
  value.title = object.value(QStringLiteral("title")).toString();
  value.id = object.value(QStringLiteral("nodeId")).toString();
  value.depth = object.value(QStringLiteral("depth")).toInt();
  value.parent = object.value(QStringLiteral("parentId")).toInt(-1);
  value.synopsis = object.value(QStringLiteral("synopsis")).toString();
  value.status = object.value(QStringLiteral("status")).toString();
  value.label = object.value(QStringLiteral("label")).toString();
  value.group = object.value(QStringLiteral("isGroup")).toBool();
  value.root = object.value(QStringLiteral("isRoot")).toBool();
  value.words = object.value(QStringLiteral("wordCount")).toInt();
  value.include = object.value(QStringLiteral("includeInCompile")).toBool();
  return &m_rows.insert(row, value).value();
}
