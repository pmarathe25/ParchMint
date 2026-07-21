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
  const auto row = source->index(sourceRow, 0, sourceParent);
  return !source->data(row, OutlineModel::RootRole).toBool();
}
