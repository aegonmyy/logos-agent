#include "agent_ffi_client.h"

#include <QByteArray>

namespace {
const char* kLib    = "liblogos_agent.so";
const char* kEnvKey = "LOGOS_AGENT_FFI_PATH";

QString errJson(const QString& message)
{
    QString escaped = message;
    escaped.replace(QLatin1Char('"'), QLatin1Char('\''));
    return QStringLiteral("{\"ok\":false,\"error\":\"%1\"}").arg(escaped);
}
} // namespace

bool AgentFfiClient::load(QString* err)
{
    if (m_loaded) {
        return true;
    }

    QString libPath = qEnvironmentVariable(kEnvKey);
    if (libPath.isEmpty()) {
        libPath = QLatin1String(kLib);
    }

    m_lib.setFileName(libPath);
    if (!m_lib.load()) {
        m_lastErr = QStringLiteral("cannot load %1: %2").arg(libPath, m_lib.errorString());
        if (err) {
            *err = m_lastErr;
        }
        return false;
    }

    m_version = reinterpret_cast<NoArgFn>(m_lib.resolve("logos_agent_version"));
    m_skills  = reinterpret_cast<NoArgFn>(m_lib.resolve("logos_agent_default_skills"));
    m_free    = reinterpret_cast<FreeFn>(m_lib.resolve("logos_agent_free_string"));

    if (!m_version || !m_skills || !m_free) {
        m_lastErr = QStringLiteral("missing symbols in %1").arg(m_lib.fileName());
        if (err) {
            *err = m_lastErr;
        }
        m_lib.unload();
        return false;
    }

    m_loaded = true;
    return true;
}

QString AgentFfiClient::callNoArg(NoArgFn fn, const char* name)
{
    char* result = fn();
    if (!result) {
        return errJson(QStringLiteral("%1 returned null").arg(QString::fromLatin1(name)));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

QString AgentFfiClient::version()
{
    if (!load()) {
        return errJson(m_lastErr);
    }
    return callNoArg(m_version, "version");
}

QString AgentFfiClient::skills()
{
    if (!load()) {
        return errJson(m_lastErr);
    }
    return callNoArg(m_skills, "skills");
}
