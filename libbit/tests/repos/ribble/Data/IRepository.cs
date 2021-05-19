using System;
using System.Threading.Tasks;

namespace RibbleChatServer.Data
{
    public interface IRepository<T>
    {
        public Task<T> FindByIdAsync(Guid id);
        public Task<T> Insert(T entity);
    }
}