using System;

namespace RibbleChatServer
{

    public interface Topic
    {
        public record Test() : Topic;
        public record NewMessage(Guid GroupId) : Topic;
    }

}