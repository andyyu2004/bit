using System;
using System.Threading.Tasks;
using RibbleChatServer.Data;
using RibbleChatServer.Models;

namespace RibbleChatServer.Services
{

    // dunno maybe a bad abstraction?
    public class UserService : IRepository<User>
    {
        private readonly MainDbContext dbContext;

        public UserService(MainDbContext dbContext)
        {
            this.dbContext = dbContext;
        }

        public async Task<User> FindByIdAsync(Guid id) => await dbContext.Users.FindAsync(id);

        public Task<User> Insert(User entity)
        {
            throw new System.NotImplementedException();
        }
    }

}