#ifndef AGENT_OWNER_INTERFACE_H
#define AGENT_OWNER_INTERFACE_H

#include <QObject>
#include <QString>

#include "interface.h"

// Marker interface for the Agent Owner Logos view plugin. All logic lives in
// AgentOwnerPlugin; this header supplies the IID used by Q_PLUGIN_METADATA and
// Q_DECLARE_INTERFACE.
class AgentOwnerInterface : public PluginInterface
{
public:
    virtual ~AgentOwnerInterface() = default;
};

#define AgentOwnerInterface_iid "org.logos.AgentOwnerInterface/1"
Q_DECLARE_INTERFACE(AgentOwnerInterface, AgentOwnerInterface_iid)

#endif // AGENT_OWNER_INTERFACE_H
