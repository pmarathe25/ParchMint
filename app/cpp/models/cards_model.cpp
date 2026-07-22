#include "cards_model.h"

#include "outline_model.h"

CardsModel::CardsModel(QObject* parent)
  : QSortFilterProxyModel(parent)
{
  setDynamicSortFilter(true);
}

QObject* CardsModel::source() const { return m_source; }

void CardsModel::setSource(QObject* source)
{
  if (m_source == source)
    return;
  m_source = source;
  setSourceModel(qobject_cast<QAbstractItemModel*>(source));
  invalidateFilter();
  emit sourceChanged();
}

bool CardsModel::filterAcceptsRow(int sourceRow, const QModelIndex& sourceParent) const
{
  const auto* source = sourceModel();
  if (!source)
    return false;
  auto row = source->index(sourceRow, 0, sourceParent);
  if (!row.isValid() || source->data(row, OutlineModel::RootRole).toBool())
    return false;

  return source->data(row, OutlineModel::RootKeyRole).toString()
    == QStringLiteral("manuscript");
}

bool CardsModel::ancestorsExpanded(int row, const QVariantMap& collapsedNodes) const
{
  const auto* source = sourceModel();
  auto current = mapToSource(index(row, 0));
  if (!source || !current.isValid())
    return false;
  int parentRow = source->data(current, OutlineModel::ParentIdRole).toInt();
  while (parentRow >= 0) {
    current = source->index(parentRow, 0);
    if (!current.isValid())
      return false;
    if (collapsedNodes.value(source->data(current, OutlineModel::IdRole).toString()).toBool())
      return false;
    parentRow = source->data(current, OutlineModel::ParentIdRole).toInt();
  }
  return true;
}
