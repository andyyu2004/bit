using System;
using System.Linq;
using HotChocolate.Data;
using HotChocolate.AspNetCore.Authorization;
using HotChocolate.Data.Filters;
using HotChocolate.Types;
using Microsoft.EntityFrameworkCore;
using RibbleChatServer.Data;
using RibbleChatServer.Models;

namespace RibbleChatServer.GraphQL
{
    public class QueryType : ObjectType<Query>
    {
        protected override void Configure(IObjectTypeDescriptor<Query> descriptor)
        {
            // https://github.com/ChilliCream/hotchocolate-docs/blob/master/docs/schema-object-type.md
            // does the type even matter?
            // descriptor
            // .Field(query => query.Users)
            // .Type<NonNullType<ListType<NonNullType<UserType>>>>();
            // .UsePaging<NonNull<UserType>>();

            // descriptor
            // .Field(query => query.Groups)
            // .Type<NonNullType<ListType<NonNullType<GroupType>>>>();
            // .UsePaging<NonNull<GroupType>>();


        }

    }

    public class UserFilteringType : FilterInputType<User>
    {
        protected override void Configure(IFilterInputTypeDescriptor<User> descriptor)
        {
        }

    }

    public class Query
    {
        private MainDbContext db;

        public Query(MainDbContext db)
        {
            this.db = db;
        }

        [UseFiltering(typeof(UserFilteringType))]
        [Authorize]
        public IQueryable<User> Users => db.Users.Include(user => user.Groups);

        [UseFiltering]
        [Authorize]
        public IQueryable<Group> Groups => db.Groups.Include(group => group.Users);
    }


}


