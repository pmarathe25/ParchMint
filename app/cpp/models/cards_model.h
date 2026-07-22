#pragma once

#include <QPointer>
#include <QSortFilterProxyModel>
#include <QVariantMap>
#include <qqmlintegration.h>

// Cards expose the manuscript hierarchy while mutations remain Rust commands.
class CardsModel : public QSortFilterProxyModel
{
  Q_OBJECT
  QML_ELEMENT
  Q_PROPERTY(QObject* source READ source WRITE setSource NOTIFY sourceChanged)

public:
  explicit CardsModel(QObject* parent = nullptr);
  QObject* source() const;
  void setSource(QObject* source);
  Q_INVOKABLE bool ancestorsExpanded(int row,
                                     const QVariantMap& collapsedNodes) const;

signals:
  void sourceChanged();

protected:
  bool filterAcceptsRow(int sourceRow, const QModelIndex& sourceParent) const override;

private:
  QPointer<QObject> m_source;
};
