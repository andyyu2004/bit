using System.Threading.Tasks;
using RibbleChatServer.Models;

namespace RibbleChatServer.Data
{
    public interface IMessageDb
    {
        public Task AddMessage(ChatMessage msg);

    }
}
