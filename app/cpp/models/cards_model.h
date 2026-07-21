#pragma once

#include <QPointer>
#include <QSortFilterProxyModel>
#include <qqmlintegration.h>

// The card grid must filter built-in roots in the model, not by hiding a
// delegate. Hidden delegates still reserve GridView cells and create gaps.
class CardsModel : public QSortFilterProxyModel
{
  Q_OBJECT
  QML_ELEMENT
  Q_PROPERTY(QObject* source READ source WRITE setSource NOTIFY sourceChanged)

public:
  explicit CardsModel(QObject* parent = nullptr);
  QObject* source() const;
  void setSource(QObject* source);

signals:
  void sourceChanged();

protected:
  bool filterAcceptsRow(int sourceRow, const QModelIndex& sourceParent) const override;

private:
  QPointer<QObject> m_source;
};
