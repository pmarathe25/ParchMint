#pragma once

#include <QObject>
#include <QHash>
#include <QPointer>
#include <QQuickTextDocument>
#include <QTextCursor>
#include <QVariantList>
#include <QVariantMap>
#include <qqmlintegration.h>

class EditorAdapter : public QObject
{
  Q_OBJECT
  QML_ELEMENT
  Q_PROPERTY(QObject* textDocument READ textDocument WRITE setTextDocument NOTIFY textDocumentChanged)
  Q_PROPERTY(int cursorPosition READ cursorPosition WRITE setCursorPosition NOTIFY cursorPositionChanged)
  Q_PROPERTY(int selectionStart READ selectionStart WRITE setSelectionStart NOTIFY selectionChanged)
  Q_PROPERTY(int selectionEnd READ selectionEnd WRITE setSelectionEnd NOTIFY selectionChanged)
  Q_PROPERTY(bool bold READ bold NOTIFY selectionFormatChanged)
  Q_PROPERTY(bool italic READ italic NOTIFY selectionFormatChanged)
  Q_PROPERTY(bool underline READ underline NOTIFY selectionFormatChanged)
  Q_PROPERTY(int boldState READ boldState NOTIFY selectionFormatChanged)
  Q_PROPERTY(int italicState READ italicState NOTIFY selectionFormatChanged)
  Q_PROPERTY(int paragraphAlignment READ paragraphAlignment NOTIFY selectionFormatChanged)
  Q_PROPERTY(QString paragraphStyle READ paragraphStyle NOTIFY selectionFormatChanged)
  Q_PROPERTY(bool canUndo READ canUndo NOTIFY undoAvailabilityChanged)
  Q_PROPERTY(bool canRedo READ canRedo NOTIFY redoAvailabilityChanged)
  Q_PROPERTY(bool focused READ focused WRITE setFocused NOTIFY focusedChanged)
  Q_PROPERTY(qulonglong revision READ revision NOTIFY revisionChanged)

public:
  explicit EditorAdapter(QObject* parent = nullptr);

  QObject* textDocument() const;
  void setTextDocument(QObject* document);
  void setDocumentForTesting(QTextDocument* document);
  int cursorPosition() const;
  void setCursorPosition(int position);
  int selectionStart() const;
  void setSelectionStart(int position);
  int selectionEnd() const;
  void setSelectionEnd(int position);
  bool bold() const;
  bool italic() const;
  bool underline() const;
  int boldState() const;
  int italicState() const;
  int paragraphAlignment() const;
  QString paragraphStyle() const;
  bool canUndo() const;
  bool canRedo() const;
  bool focused() const;
  void setFocused(bool focused);
  qulonglong revision() const;

  Q_INVOKABLE void loadSemanticBlocks(const QVariantList& blocks);
  Q_INVOKABLE QVariantList semanticBlocks() const;
  Q_INVOKABLE void toggleBold();
  Q_INVOKABLE void toggleItalic();
  Q_INVOKABLE void toggleUnderline();
  Q_INVOKABLE void setVerticalAlignment(int alignment);
  Q_INVOKABLE void setParagraphAlignment(int alignment);
  Q_INVOKABLE void setHeadingLevel(int level);
  Q_INVOKABLE void defineStyle(const QString& styleId,
                               const QVariantMap& properties,
                               bool paragraph,
                               const QString& nextStyleId = QString());
  Q_INVOKABLE void setParagraphStyle(const QString& styleId);
  Q_INVOKABLE void setCharacterStyle(const QString& styleId);
  Q_INVOKABLE void clearDirectFormatting();
  Q_INVOKABLE void toggleList(bool ordered);
  Q_INVOKABLE void setLink(const QString& destination);
  Q_INVOKABLE void insertImage(const QString& assetId, const QString& altText);
  Q_INVOKABLE void insertOpaqueBlock(const QString& source, const QString& reason);
  Q_INVOKABLE void insertParagraphBreak();
  Q_INVOKABLE void insertPageBreak();
  Q_INVOKABLE void insertSceneBreak();
  Q_INVOKABLE void deletePreviousSemanticUnit();
  Q_INVOKABLE void deleteNextSemanticUnit();
  Q_INVOKABLE void pastePlainText(const QString& text);
  Q_INVOKABLE void pasteRichHtml(const QString& html);
  Q_INVOKABLE void undo();
  Q_INVOKABLE void redo();
  Q_INVOKABLE void beginGroupedEdit();
  Q_INVOKABLE void endGroupedEdit();
  Q_INVOKABLE QString selectedPlainText() const;

signals:
  void textDocumentChanged();
  void cursorPositionChanged();
  void selectionChanged();
  void selectionFormatChanged();
  void undoAvailabilityChanged();
  void redoAvailabilityChanged();
  void focusedChanged();
  void revisionChanged();
  void incrementalDirty(qulonglong revision,
                        int position,
                        int removed,
                        int added,
                        int firstBlock,
                        int lastBlockExclusive);
  void focusLostFlushRequested(qulonglong revision);
  void adapterError(const QString& message);

private:
  QTextCursor cursor() const;
  void mergeCharacterFormat(const QTextCharFormat& format);
  bool canReplaceSelection(const QTextCursor& cursor, const QString& action);
  void applyNextParagraphStyle(QTextCursor& cursor, const QString& currentStyle);
  int uniformCharacterProperty(int property, const QVariant& enabledValue) const;
  bool selectionContainsProtectedObject(const QTextCursor& cursor) const;
  void connectDocumentSignals();
  void onContentsChange(int position, int removed, int added);
  static int clampPosition(const QTextDocument* document, int position);

  QPointer<QQuickTextDocument> m_textDocument;
  QPointer<QTextDocument> m_document;
  int m_cursorPosition = 0;
  int m_selectionStart = 0;
  int m_selectionEnd = 0;
  bool m_groupedEdit = false;
  bool m_loading = false;
  bool m_focused = false;
  qulonglong m_revision = 0;
  QObject* m_objectRenderer = nullptr;
  QHash<QString, QVariantMap> m_styleDefinitions;
  QHash<QString, bool> m_paragraphStyles;
  QHash<QString, QString> m_nextStyles;
};
