#pragma once

#include <QObject>
#include <QPointer>
#include <QQuickTextDocument>
#include <QTextCursor>
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

  Q_INVOKABLE void toggleBold();
  Q_INVOKABLE void toggleItalic();
  Q_INVOKABLE void setVerticalAlignment(int alignment);
  Q_INVOKABLE void setParagraphAlignment(int alignment);
  Q_INVOKABLE void insertPageBreak();
  Q_INVOKABLE void insertSceneBreak();
  Q_INVOKABLE void beginGroupedEdit();
  Q_INVOKABLE void endGroupedEdit();
  Q_INVOKABLE QString selectedPlainText() const;

signals:
  void textDocumentChanged();
  void cursorPositionChanged();
  void selectionChanged();
  void selectionFormatChanged();
  void adapterError(const QString& message);

private:
  QTextCursor cursor() const;
  void mergeCharacterFormat(const QTextCharFormat& format);
  static int clampPosition(const QTextDocument* document, int position);

  QPointer<QQuickTextDocument> m_textDocument;
  QPointer<QTextDocument> m_document;
  int m_cursorPosition = 0;
  int m_selectionStart = 0;
  int m_selectionEnd = 0;
  bool m_groupedEdit = false;
};
