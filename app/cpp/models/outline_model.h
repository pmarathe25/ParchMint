#pragma once

#include <QAbstractListModel>
#include <QPointer>
#include <qqmlintegration.h>

class OutlineModel : public QAbstractListModel
{
  Q_OBJECT
  QML_ELEMENT
  Q_PROPERTY(QObject* source READ source WRITE setSource NOTIFY sourceChanged)

public:
  enum Role {
    TitleRole = Qt::UserRole + 1,
    IdRole,
    DepthRole,
    ParentIdRole,
    SynopsisRole,
    StatusRole,
    LabelRole,
    GroupRole,
    RootRole,
  };
  Q_ENUM(Role)

  explicit OutlineModel(QObject* parent = nullptr);
  QObject* source() const;
  void setSource(QObject* source);
  int rowCount(const QModelIndex& parent = QModelIndex()) const override;
  QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
  QHash<int, QByteArray> roleNames() const override;

signals:
  void sourceChanged();
  void modelError(const QString& message) const;

private slots:
  void refresh();

private:
  QVariant invoke(const char* method, int row) const;
  QPointer<QObject> m_source;
  QMetaObject::Connection m_revisionConnection;
};
