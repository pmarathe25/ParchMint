#pragma once

#include <QAbstractListModel>
#include <QHash>
#include <QMimeData>
#include <QPointer>
#include <QVariantMap>
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
    ParentNodeIdRole,
    RootKeyRole,
    SynopsisRole,
    StatusRole,
    LabelRole,
    GroupRole,
    RootRole,
    HasChildrenRole,
    WordCountRole,
    IncludeInCompileRole,
  };
  Q_ENUM(Role)

  explicit OutlineModel(QObject* parent = nullptr);
  QObject* source() const;
  void setSource(QObject* source);
  Q_INVOKABLE bool ancestorsExpanded(int row,
                                     const QVariantMap& collapsedNodes) const;
  int rowCount(const QModelIndex& parent = QModelIndex()) const override;
  QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
  QHash<int, QByteArray> roleNames() const override;
  Qt::ItemFlags flags(const QModelIndex& index) const override;
  QStringList mimeTypes() const override;
  QMimeData* mimeData(const QModelIndexList& indexes) const override;
  bool dropMimeData(const QMimeData* data,
                    Qt::DropAction action,
                    int row,
                    int column,
                    const QModelIndex& parent) override;

signals:
  void sourceChanged();
  void modelError(const QString& message) const;

private slots:
  void applyDelta(const QString& kind, int first, int destination, int count);

private:
  struct CachedRow
  {
    QString title;
    QString id;
    QString parentNodeId;
    QString rootKey;
    int depth = 0;
    int parent = -1;
    QString synopsis;
    QString status;
    QString label;
    bool group = false;
    bool root = false;
    bool hasChildren = false;
    int words = 0;
    bool include = false;
  };
  const CachedRow* cachedRow(int row) const;
  QPointer<QObject> m_source;
  QMetaObject::Connection m_revisionConnection;
  mutable QHash<int, CachedRow> m_rows;
  int m_rowCount = 0;
};
