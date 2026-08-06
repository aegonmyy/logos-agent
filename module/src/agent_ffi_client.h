#ifndef AGENT_FFI_CLIENT_H
#define AGENT_FFI_CLIENT_H

#include <QLibrary>
#include <QString>

// Lazy-loads liblogos_agent.so (the Rust agent core) and exposes its C
// entry-points. The library path can be overridden with LOGOS_AGENT_FFI_PATH;
// otherwise the loader searches the default library name on the system path.
class AgentFfiClient
{
public:
    QString version();
    QString skills();
    QString lastError() const { return m_lastErr; }

private:
    using NoArgFn = char* (*)();
    using FreeFn  = void  (*)(char*);

    bool load(QString* err = nullptr);
    QString callNoArg(NoArgFn fn, const char* name);

    QLibrary m_lib;
    NoArgFn  m_version = nullptr;
    NoArgFn  m_skills  = nullptr;
    FreeFn   m_free    = nullptr;
    bool     m_loaded  = false;
    QString  m_lastErr;
};

#endif // AGENT_FFI_CLIENT_H
